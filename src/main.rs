#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]

use std::collections::HashMap;
use std::fmt::{Debug, Display};

use breadx::{
    prelude::{AsyncDisplayXprotoExt, MapState, SetMode},
    traits::DisplayBase,
    AsyncDisplay, AsyncDisplayConnection, AsyncDisplayExt, BreadError, ConfigureWindowParameters,
    Event, EventMask, Window,
};

mod x11;

use x11::client::XcrabClient;

const BORDER_WIDTH: u16 = 5;
const GAP_WIDTH: u16 = 10;

#[non_exhaustive]
pub enum XcrabError {
    Bread(BreadError),
}

impl From<BreadError> for XcrabError {
    fn from(v: BreadError) -> Self {
        Self::Bread(v)
    }
}

impl Display for XcrabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bread(be) => Display::fmt(&be, f)?,
        };
        Ok(())
    }
}

impl Debug for XcrabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

#[tokio::main]
async fn main() -> Result<(), XcrabError> {
    // connect to the x server
    let mut conn = AsyncDisplayConnection::create_async(None, None).await?;

    let root = conn.default_root();

    // listen for substructure redirects to intercept events like window creation
    root.set_event_mask_async(
        &mut conn,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
    )
    .await?;

    let mut clients = HashMap::new();

    conn.grab_server_async().await?;

    let top_level_windows = root.query_tree_immediate_async(&mut conn).await?.children;

    for &win in top_level_windows.iter() {
        let attrs = win.window_attributes_immediate_async(&mut conn).await?;

        println!("a");
        if !attrs.override_redirect && attrs.map_state == MapState::Viewable {
            clients.insert(
                win,
                XcrabClient::new(win, &mut conn, clients.len() + 1).await?,
            );
            x11::client::calculate_geometry(&mut clients, &mut conn).await?;
        }
    }

    conn.ungrab_server_async().await?;

    loop {
        let ev = conn.wait_for_event_async().await?;

        match ev {
            Event::MapRequest(ev) => {
                let win = ev.window;

                clients.insert(
                    win,
                    XcrabClient::new(win, &mut conn, clients.len() + 1).await?,
                );
                x11::client::calculate_geometry(&mut clients, &mut conn).await?;
            }
            Event::ConfigureRequest(ev) => {
                // cope from `ev` to `params`
                let mut params = ConfigureWindowParameters {
                    x: Some(ev.x.into()),
                    y: Some(ev.y.into()),
                    width: Some(ev.width.into()),
                    height: Some(ev.height.into()),
                    border_width: Some(ev.border_width.into()),
                    sibling: Some(ev.sibling),
                    stack_mode: Some(ev.stack_mode),
                };

                // if this is a client, deny changing position or size (we are a tiling wm!)
                if clients.contains_key(&ev.window) {
                    params.x = None;
                    params.y = None;
                    params.width = None;
                    params.height = None;
                }

                // forward the request
                ev.window.configure_async(&mut conn, params).await?;
            }
            Event::UnmapNotify(ev) => {
                if ev.event != root {
                    if let Some(parent) = clients.get(&ev.window) {
                        parent.parent.unmap_async(&mut conn).await?;

                        ev.window.reparent_async(&mut conn, root, 0, 0).await?;

                        // no longer related to us, remove from save set
                        ev.window
                            .change_save_set_async(&mut conn, SetMode::Delete)
                            .await?;

                        parent.parent.free_async(&mut conn).await?;

                        clients.remove(&ev.window);
                    }
                }
            }
            _ => {}
        }
    }
}

async fn manage_window<Dpy: AsyncDisplay + ?Sized>(
    conn: &mut Dpy,
    win: Window,
) -> Result<Window, XcrabError> {
    // the client wishes for their window to be displayed. we must create a new
    // window with a titlebar and reparent the old window to this new window.

    let root = conn.default_root();

    let geometry = win.geometry_immediate_async(conn).await?;

    // TODO: tiling window manager logic
    let new_x = geometry.x;
    let new_y = geometry.y;
    let new_width = geometry.width;
    let new_height = geometry.height;

    let parent = conn
        .create_simple_window_async(
            root, new_x, new_y, new_width, new_height, 3, 0xff_00_00, 0x00_00_00,
        )
        .await?;

    parent
        .set_event_mask_async(
            conn,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        )
        .await?;

    win.change_save_set_async(conn, SetMode::Insert).await?;

    // tell the window what size we made it
    win.configure_async(
        conn,
        ConfigureWindowParameters {
            width: Some(new_width.into()),
            height: Some(new_height.into()),
            ..breadx::ConfigureWindowParameters::default()
        },
    )
    .await?;

    win.reparent_async(conn, parent, 0, 0).await?;

    parent.map_async(conn).await?;

    win.map_async(conn).await?;

    Ok(parent)
}
