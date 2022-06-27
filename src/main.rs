#![warn(clippy::pedantic)]

use std::fmt::{Debug, Display};

use breadx::{
    prelude::{AsyncDisplayXprotoExt, MapState},
    traits::DisplayBase,
    AsyncDisplayConnection, AsyncDisplayExt, BreadError, ConfigureWindowParameters, Event,
    EventMask,
};

use lazy_static::lazy_static;

mod config;
mod x11;

use x11::client::{may_not_exist, XcrabWindowManager};

#[non_exhaustive]
pub enum XcrabError {
    Bread(BreadError),
    Io(std::io::Error),
    Toml(toml::de::Error),
    Var(std::env::VarError),
    ClientDoesntExist,
}

impl From<BreadError> for XcrabError {
    fn from(v: BreadError) -> Self {
        Self::Bread(v)
    }
}

impl From<std::io::Error> for XcrabError {
    fn from(v: std::io::Error) -> Self {
        Self::Io(v)
    }
}

impl From<toml::de::Error> for XcrabError {
    fn from(v: toml::de::Error) -> Self {
        Self::Toml(v)
    }
}

impl From<std::env::VarError> for XcrabError {
    fn from(v: std::env::VarError) -> Self {
        Self::Var(v)
    }
}

lazy_static! {
    pub static ref CONFIG: config::XcrabConfig = config::load_file().unwrap_or_default();
}

impl Display for XcrabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bread(be) => Display::fmt(&be, f)?,
            Self::Io(ie) => Display::fmt(&ie, f)?,
            Self::Toml(te) => Display::fmt(&te, f)?,
            Self::Var(ve) => Display::fmt(&ve, f)?,
            Self::ClientDoesntExist => Display::fmt("client didn't exist", f)?,
        };

        Ok(())
    }
}

impl Debug for XcrabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

type Result<T> = std::result::Result<T, XcrabError>;

#[tokio::main]
async fn main() -> Result<()> {
    // connect to the x server
    let mut conn = AsyncDisplayConnection::create_async(None, None).await?;

    let root = conn.default_root();

    // listen for substructure redirects to intercept events like window creation
    root.set_event_mask_async(
        &mut conn,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
    )
    .await?;

    let mut manager = XcrabWindowManager::new();

    conn.grab_server_async().await?;

    let top_level_windows = root.query_tree_immediate_async(&mut conn).await?.children;

    for &win in top_level_windows.iter() {
        let attrs = win.window_attributes_immediate_async(&mut conn).await?;

        if !attrs.override_redirect && attrs.map_state == MapState::Viewable {
            manager.add_client(&mut conn, win).await?;
        }
    }

    conn.ungrab_server_async().await?;

    loop {
        let ev = conn.wait_for_event_async().await?;

        match ev {
            Event::MapRequest(ev) => {
                manager.add_client(&mut conn, ev.window).await?;
            }
            Event::ConfigureRequest(ev) => {
                // copy from `ev` to `params`
                let mut params = ConfigureWindowParameters {
                    x: Some(ev.x.into()),
                    y: Some(ev.y.into()),
                    width: Some(ev.width.into()),
                    height: Some(ev.height.into()),
                    border_width: Some(ev.border_width.into()),
                    // without this, it will error when a window tries to set a sibling that is
                    // not actually a sibling? idk, all i know is that without it, xterm crashes
                    sibling: None,
                    stack_mode: Some(ev.stack_mode),
                };

                // if this is a client, deny changing position or size (we are a tiling wm!)
                if manager.has_client(ev.window) {
                    params.x = None;
                    params.y = None;
                    params.width = None;
                    params.height = None;
                }

                // forward the request
                // by the time we get here someone may have already deleted their window (xterm, im looking at you!)
                may_not_exist(ev.window.configure_async(&mut conn, params).await)?;
            }
            Event::UnmapNotify(ev) => {
                if ev.event != root && manager.has_client(ev.window) {
                    manager.remove_client(&mut conn, ev.window).await?;
                }
            }
            _ => {}
        }
    }
}
