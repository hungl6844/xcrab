use breadx::prelude::{AsyncDisplayXprotoExt, SetMode};
use breadx::{AsyncDisplay, ConfigureWindowParameters, EventMask, Window};
use std::collections::HashMap;

use crate::{XcrabError, Result};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Default)]
pub struct XcrabWindowManager {
    clients: HashMap<Window, XcrabClient>,
    focused: Option<Window>,
    grid_width: u32,
    grid_height: u32,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Copy)]
struct XcrabClient {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl XcrabWindowManager {
    pub fn new() -> Self {
        XcrabWindowManager::default()
    }

    pub async fn add_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        self.add_client_direction(conn, win, Direction::Right).await
    }

    pub async fn add_client_direction<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
        direction: Direction,
    ) -> Result<()> {
        let frame = frame(conn, win).await?;

        if let Some(focused) = self.focused {
            use Direction::*;

            let focused_client = *self.clients.get(&focused).ok_or(XcrabError::ClientDoesntExist)?;

            let new_client = match direction {
                Up | Down => {
                    if focused_client.height % 2 == 1 {
                        for client in self.clients.values_mut() {
                            client.y *= 2;
                            client.height *= 2;
                        }

                        self.grid_height *= 2;
                    }

                    let focused_client = self.clients.get_mut(&focused).unwrap();

                    focused_client.height /= 2;
                    let height = focused_client.height;

                    let mut new_client = *focused_client;

                    if let Up = direction {
                        focused_client.y += height;
                    } else {
                        new_client.y += height;
                    }

                    new_client
                },
                Left | Right => {
                    if focused_client.width % 2 == 1 {
                        for client in self.clients.values_mut() {
                            client.x *= 2;
                            client.width *= 2;
                        }

                        self.grid_width *= 2;
                    }

                    let focused_client = self.clients.get_mut(&focused).unwrap();

                    focused_client.width /= 2;
                    let width = focused_client.width;

                    let mut new_client = *focused_client;

                    if let Left = direction {
                        focused_client.x += width;
                    } else {
                        new_client.x += width;
                    }

                    new_client
                }
            };
        
            self.clients.insert(win, new_client);

            self.update_client(conn, focused).await?;
            self.update_client(conn, win).await?;

            self.focused = Some(win);
        } else {
            self.grid_width = 1;
            self.grid_height = 1;

            let client = XcrabClient {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            };

            self.clients.insert(win, client);

            self.update_client(conn, win).await?;

            self.focused = Some(win);
        }

        win.map_async(conn).await?;
        frame.map_async(conn).await?;

        Ok(())
    }

    pub async fn update_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        let client = *self.clients.get(&win).ok_or(XcrabError::ClientDoesntExist)?;

        let root = conn.default_root();
        let root_geo = root.geometry_immediate_async(conn).await?;
        let root_width: u32 = root_geo.width.into();
        let root_height: u32 = root_geo.height.into();
        
        let x = (client.x * root_width) / self.grid_width;
        let y = (client.y * root_height) / self.grid_height;
        let width = (client.width * root_width) / self.grid_width;
        let height = (client.height * root_height) / self.grid_height;

        let parent = win.query_tree_immediate_async(conn).await?.parent;

        parent.configure_async(conn, ConfigureWindowParameters {
            x: Some(x.try_into().unwrap()),
            y: Some(y.try_into().unwrap()),
            width: Some(width),
            height: Some(height),
            ..Default::default()
        }).await?;

        win.configure_async(conn, ConfigureWindowParameters {
            x: Some(0),
            y: Some(0),
            width: Some(width),
            height: Some(height),
            ..Default::default()
        }).await?;

        Ok(())
    }

    pub fn has_client(&self, win: Window) -> bool {
        self.clients.contains_key(&win)
    }

    pub async fn remove_client<Dpy: AsyncDisplay + ?Sized>(&mut self, conn: &mut Dpy, win: Window) -> Result<()> {
        // TODO: maybe an `unframe` method?
        let root = conn.default_root();

        let parent = win.query_tree_immediate_async(conn).await?.parent;

        parent.unmap_async(conn).await?;

        win.reparent_async(conn, root, 0, 0).await?;

        // no longer related to us, remove from save set
        win
            .change_save_set_async(conn, SetMode::Delete)
            .await?;

        parent.free_async(conn).await?;

        self.clients.remove(&win);

        Ok(())
    }
}

async fn frame<Dpy: AsyncDisplay + ?Sized>(
    conn: &mut Dpy,
    win: Window,
) -> Result<Window> {
    const BORDER_WIDTH: u16 = 3;
    const GAP_WIDTH: u16 = 10;

    let root = conn.default_root();

    let geometry = win.geometry_immediate_async(conn).await?;

    let parent = conn
        .create_simple_window_async(
            root,
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height,
            3,
            0xff_00_00,
            0x00_00_00,
        )
        .await?;

    parent
        .set_event_mask_async(
            conn,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        )
        .await?;

    win.change_save_set_async(conn, SetMode::Insert).await?;

    win.reparent_async(conn, parent, 0, 0).await?;

    Ok(parent)
}
