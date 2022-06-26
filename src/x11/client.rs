use breadx::prelude::{AsyncDisplayXprotoExt, SetMode};
use breadx::{AsyncDisplay, ConfigureWindowParameters, EventMask, Window};
use std::collections::HashMap;

use crate::{Result, XcrabError, CONFIG};

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
    frame: FramedWindow,
    geo: XcrabGeometry,
}

#[derive(Debug, Clone, Copy)]
struct XcrabGeometry {
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
            #[allow(clippy::enum_glob_use)]
            use Direction::*;

            let focused_client = *self
                .clients
                .get(&focused)
                .ok_or(XcrabError::ClientDoesntExist)?;

            let new_client = match direction {
                Up | Down => {
                    if focused_client.geo.height % 2 == 1 {
                        for client in self.clients.values_mut() {
                            client.geo.y *= 2;
                            client.geo.height *= 2;
                        }

                        self.grid_height *= 2;
                    }

                    let focused_client = self.clients.get_mut(&focused).unwrap();

                    focused_client.geo.height /= 2;
                    let height = focused_client.geo.height;

                    let mut new_client = XcrabClient { frame, geo: focused_client.geo };

                    if let Up = direction {
                        focused_client.geo.y += height;
                    } else {
                        new_client.geo.y += height;
                    }

                    new_client
                }
                Left | Right => {
                    if focused_client.geo.width % 2 == 1 {
                        for client in self.clients.values_mut() {
                            client.geo.x *= 2;
                            client.geo.width *= 2;
                        }

                        self.grid_width *= 2;
                    }

                    let focused_client = self.clients.get_mut(&focused).unwrap();

                    focused_client.geo.width /= 2;
                    let width = focused_client.geo.width;

                    let mut new_client = XcrabClient { frame, geo: focused_client.geo };

                    if let Left = direction {
                        focused_client.geo.x += width;
                    } else {
                        new_client.geo.x += width;
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

            let geo = XcrabGeometry {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            };

            let client = XcrabClient { frame, geo };

            self.clients.insert(win, client);

            self.update_client(conn, win).await?;

            self.focused = Some(win);
        }

        frame.map(conn).await?;

        Ok(())
    }

    pub async fn update_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        let client = *self
            .clients
            .get(&win)
            .ok_or(XcrabError::ClientDoesntExist)?;

        let root = conn.default_root();
        let root_geo = root.geometry_immediate_async(conn).await?;
        let root_width: u32 = root_geo.width.into();
        let root_height: u32 = root_geo.height.into();

        let x = (client.geo.x * root_width) / self.grid_width;
        let y = (client.geo.y * root_height) / self.grid_height;
        let width = (client.geo.width * root_width) / self.grid_width;
        let height = (client.geo.height * root_height) / self.grid_height;

        client
            .frame
            .configure(
                conn,
                ConfigureWindowParameters {
                    x: Some(x.try_into().unwrap()),
                    y: Some(y.try_into().unwrap()),
                    width: Some(width),
                    height: Some(height),
                    ..Default::default()
                },
            )
            .await?;

        Ok(())
    }

    pub fn has_client(&self, win: Window) -> bool {
        self.clients.contains_key(&win)
    }

    pub async fn remove_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        self.clients
            .remove(&win)
            .ok_or(XcrabError::ClientDoesntExist)?
            .frame
            .unframe(conn)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct FramedWindow {
    frame: Window,
    win: Window,
}

impl FramedWindow {
    async fn configure<Dpy: AsyncDisplay + ?Sized>(
        self,
        conn: &mut Dpy,
        props: ConfigureWindowParameters,
    ) -> Result<()> {
        let border_size = CONFIG.border_size();
        let gap_size = CONFIG.gap_size();

        let coordinate_inset = i32::from(gap_size);
        let dimension_inset = 2 * (u32::from(gap_size) + u32::from(border_size));

        let width = props.width.map(|v| v - dimension_inset);
        let height = props.height.map(|v| v - dimension_inset);

        self.frame
            .configure_async(
                conn,
                ConfigureWindowParameters {
                    x: props.x.map(|v| v + coordinate_inset),
                    y: props.y.map(|v| v + coordinate_inset),
                    width,
                    height,
                    border_width: Some(border_size.into()),
                    ..Default::default()
                },
            )
            .await?;

        self.win
            .configure_async(
                conn,
                ConfigureWindowParameters {
                    x: Some(-1),
                    y: Some(-1),
                    width,
                    height,
                    ..Default::default()
                },
            )
            .await?;

        Ok(())
    }

    async fn map<Dpy: AsyncDisplay + ?Sized>(self, conn: &mut Dpy) -> Result<()> {
        self.win.map_async(conn).await?;
        self.frame.map_async(conn).await?;

        Ok(())
    }

    async fn unframe<Dpy: AsyncDisplay + ?Sized>(self, conn: &mut Dpy) -> Result<()> {
        let root = conn.default_root();

        self.frame.unmap_async(conn).await?;

        self.win.reparent_async(conn, root, 0, 0).await?;

        // no longer related to us, remove from save set
        self.win
            .change_save_set_async(conn, SetMode::Delete)
            .await?;

        self.frame.free_async(conn).await?;

        Ok(())
    }
}

async fn frame<Dpy: AsyncDisplay + ?Sized>(conn: &mut Dpy, win: Window) -> Result<FramedWindow> {
    let root = conn.default_root();

    let geometry = win.geometry_immediate_async(conn).await?;

    let frame = conn
        .create_simple_window_async(
            root,
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height,
            CONFIG.border_size(),
            CONFIG.border_color(),
            0x00_00_00,
        )
        .await?;

    frame
        .set_event_mask_async(
            conn,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        )
        .await?;

    win.change_save_set_async(conn, SetMode::Insert).await?;

    win.reparent_async(conn, frame, 0, 0).await?;

    Ok(FramedWindow { frame, win })
}
