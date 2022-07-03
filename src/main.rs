// Copyright (C) 2022 Infoshock Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![warn(clippy::pedantic)]

use std::fmt::{Debug, Display};
use std::ops::Deref;

use breadx::{
    prelude::{AsyncDisplay, AsyncDisplayXprotoExt, MapState},
    traits::DisplayBase,
    AsyncDisplayConnection, AsyncDisplayExt, BreadError, ConfigureWindowParameters, Event,
    EventMask, Window,
};

use lazy_static::lazy_static;

use tokio::sync::mpsc::unbounded_channel;

mod config;
mod msg_listener;
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

    let (send, mut recv) = unbounded_channel();

    tokio::spawn(msg_listener::listener_task(
        CONFIG.msg.clone().unwrap_or_default().socket_path,
        send,
    ));

    loop {
        // biased mode makes select! poll the channel first in order to keep xcrab-msg from being
        // starved by x11 events. Probably unnecessary, but better safe than sorry.
        tokio::select! {
            biased;
            Some(s) = recv.recv() => msg_listener::on_recv(s, &mut manager, &mut conn).await?,
            Ok(ev) = conn.wait_for_event_async() => process_event(ev, &mut manager, &mut conn, root).await?,
        }
    }
}

async fn process_event<Dpy: AsyncDisplay + ?Sized>(
    ev: Event,
    manager: &mut XcrabWindowManager,
    conn: &mut Dpy,
    root: Window,
) -> Result<()> {
    match ev {
        Event::MapRequest(ev) => {
            manager.add_client(conn, ev.window).await?;
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
            // by the time we get here someone may have already deleted their window
            may_not_exist(ev.window.configure_async(conn, params).await)?;
        }
        Event::UnmapNotify(ev) => {
            if ev.event != root && manager.has_client(ev.window) {
                manager.remove_client(conn, ev.window).await?;
            }
        }
        Event::ButtonPress(ev) => {
            dbg!(&ev);
            if ev.detail == 1 {
                manager.set_focus(conn, ev.event).await?;
            }
        }
        _ => {}
    }
    Ok(())
}
