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

use breadx::prelude::{AsyncDisplayXprotoExt, SetMode};
use breadx::{
    AsyncDisplay, BreadError, ConfigureWindowParameters, ErrorCode, EventMask, Window,
    WindowParameters,
};
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::{Result, XcrabError, CONFIG};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directionality {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Dimensions {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Dimensions {
    fn split(self, direction: Directionality, count: usize) -> Vec<Self> {
        match direction {
            Directionality::Horizontal => {
                let excess = self.width % u16::try_from(count).unwrap();
                let new_width = self.width / u16::try_from(count).unwrap();

                (0..count.try_into().unwrap())
                    .map(|i| Dimensions {
                        x: self.x + i * new_width + if i < excess { 1 } else { 0 },
                        width: new_width,
                        ..self
                    })
                    .collect()
            }
            Directionality::Vertical => {
                let excess = self.height % u16::try_from(count).unwrap();
                let new_height = self.height / u16::try_from(count).unwrap();

                (0..count.try_into().unwrap())
                    .map(|i| Dimensions {
                        y: self.y + i * new_height + if i < excess { 1 } else { 0 },
                        height: new_height,
                        ..self
                    })
                    .collect()
            }
        }
    }
}

new_key_type!(
    struct XcrabKey;
);

#[derive(Debug, Clone, Default)]
pub struct XcrabWindowManager {
    clients: HashMap<Window, XcrabKey>,
    rects: SlotMap<XcrabKey, Rectangle>,
    focused: Option<Window>,
}

#[derive(Debug, Clone)]
struct Rectangle {
    parent: XcrabKey,
    cached_dimensions: Dimensions,
    contents: RectangleContents,
}

impl Rectangle {
    fn unwrap_pane(&self) -> &Pane {
        match &self.contents {
            RectangleContents::Pane(pane) => pane,
            RectangleContents::Client(_) => unreachable!(),
        }
    }

    fn unwrap_client(&self) -> &Client {
        match &self.contents {
            RectangleContents::Pane(_) => unreachable!(),
            RectangleContents::Client(client) => client,
        }
    }

    fn unwrap_pane_mut(&mut self) -> &mut Pane {
        match &mut self.contents {
            RectangleContents::Pane(pane) => pane,
            RectangleContents::Client(_) => unreachable!(),
        }
    }

    fn unwrap_client_mut(&mut self) -> &mut Client {
        match &mut self.contents {
            RectangleContents::Pane(_) => unreachable!(),
            RectangleContents::Client(client) => client,
        }
    }
}

#[derive(Debug, Clone)]
enum RectangleContents {
    Pane(Pane),
    Client(Client),
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
struct Pane {
    children: Vec<XcrabKey>,
    directionality: Directionality,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Copy)]
struct Client {
    frame: FramedWindow,
}

impl XcrabWindowManager {
    pub fn new() -> Self {
        XcrabWindowManager::default()
    }

    /// Given the `rect_key` from a `parent -> rect` relationship, makes A
    /// `parent -> new_pane -> rect` relationship, then returns `new_pane_key`
    fn insert_pane_above(
        &mut self,
        rect_key: XcrabKey,
        directionality: Directionality,
    ) -> Option<XcrabKey> {
        let rect = self.rects.get(rect_key)?;
        let rect_dimensions = rect.cached_dimensions;
        let parent_key = rect.parent;

        let new_pane = Rectangle {
            parent: parent_key,
            cached_dimensions: rect_dimensions,
            contents: RectangleContents::Pane(Pane {
                children: vec![rect_key],
                directionality,
            }),
        };

        let new_pane_key = if parent_key == rect_key {
            // the given node was the root node

            // this new pane will be the new root, so it becomes its own parent
            self.rects.insert_with_key(|key| Rectangle {
                parent: key,
                ..new_pane
            })
        } else {
            // the given node was not the root node, and thus has a parent

            let new_pane_key = self.rects.insert(new_pane);

            let parent_pane = self.rects.get_mut(parent_key).unwrap().unwrap_pane_mut();
            let index = parent_pane
                .children
                .iter()
                .copied()
                .position(|v| v == rect_key)
                .unwrap();
            // replace the "parent -> rect" relationship with a "parent -> new_pane" relationship
            parent_pane.children[index] = new_pane_key;

            new_pane_key
        };

        let rect = self.rects.get_mut(rect_key).unwrap();
        rect.parent = new_pane_key;

        Some(new_pane_key)
    }

    /// Adds a new client.
    pub async fn add_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        // use rand::prelude::SliceRandom;
        // let direction = *[
        //     Direction::Up,
        //     Direction::Down,
        //     Direction::Left,
        //     Direction::Right,
        // ]
        // .choose(&mut rand::thread_rng())
        // .unwrap();
        self.add_client_direction(conn, win, Direction::Right).await
    }

    /// Adds a new client in the given direction from the focused window.
    pub async fn add_client_direction<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
        direction: Direction,
    ) -> Result<()> {
        #[allow(clippy::enum_glob_use)]
        use {Direction::*, Directionality::*};

        let focused = match self.focused {
            Some(v) => v,
            None => return self.add_first_client(conn, win).await,
        };

        // this code path is somewhat difficult to understand, so i added some comments

        // frame the window
        let frame = frame(conn, win).await?;

        // the XcrabKey to the focused client
        let focused_client_key = *self
            .clients
            .get(&focused)
            .ok_or(XcrabError::ClientDoesntExist)?;

        // the directionality we want to find: if we are tiling Up or Down, we
        // want a Vertical pane, and for Left or Right we want a Horizontal one.
        let target_directionality = match direction {
            Up | Down => Vertical,
            Left | Right => Horizontal,
        };

        // this var will be used in the upcoming loop
        let mut child_key = focused_client_key;

        // go up the chain (using `Rectangle.parent`) until you find a pane with the correct directionality
        let parent_key = loop {
            let parent_key = self.rects.get(child_key).unwrap().parent;

            if parent_key == child_key {
                // uh oh, we hit the top, now we will wrap the root client
                // in a new pane and make this new pane the root

                break self
                    .insert_pane_above(child_key, target_directionality)
                    .unwrap();
            }

            let parent = self.rects.get(parent_key).unwrap();

            if parent.unwrap_pane().directionality == target_directionality {
                // yay! found it
                break parent_key;
            }

            // nope, continue
            child_key = parent_key;
        };

        // `parent_key` now holds the key for the pane with the target
        // directionality, and `child_key` holds the child key which will
        // be used to find where to insert our new client

        // the key to the newly created client
        let new_rect_key = self.rects.insert(Rectangle {
            parent: parent_key,
            // this default will be overriden by the `update_rectangle` down below
            cached_dimensions: Dimensions::default(),
            contents: RectangleContents::Client(Client { frame }),
        });

        // the Pane of the Rectangle of `parent_key`
        let parent_pane = self.rects.get_mut(parent_key).unwrap().unwrap_pane_mut();

        // the index which we want to `insert` at, found using `child_key`
        let mut index = parent_pane
            .children
            .iter()
            .copied()
            .position(|v| v == child_key)
            .unwrap();

        if let Down | Right = direction {
            index += 1;
        }

        // insert the new rect
        parent_pane.children.insert(index, new_rect_key);

        self.clients.insert(win, new_rect_key);

        self.focused = Some(win);

        // update the parent rectangle to also update all the siblings of our new rect
        self.update_rectangle(conn, parent_key, None).await?;

        frame.map(conn).await?;

        Ok(())
    }

    /// Adds a new client in the given direction directly adjacent to the focused window, creating a new pane if needed.
    pub async fn add_client_direction_immediate<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
        direction: Direction,
    ) -> Result<()> {
        #[allow(clippy::enum_glob_use)]
        use {Direction::*, Directionality::*};

        let focused = match self.focused {
            Some(v) => v,
            None => return self.add_first_client(conn, win).await,
        };

        // frame the window
        let frame = frame(conn, win).await?;

        // get the focused client
        let focused_client_key = *self.clients.get(&focused).unwrap();
        let focused_client = self.rects.get(focused_client_key).unwrap();

        // get the parent of the focused client
        let mut parent_key = focused_client.parent;
        let parent_pane_dir = match &self.rects.get(parent_key).unwrap().contents {
            RectangleContents::Pane(pane) => Some(pane.directionality),
            RectangleContents::Client(_) => None,
        };

        // find the target directionality
        let target_directionality = match direction {
            Up | Down => Vertical,
            Left | Right => Horizontal,
        };

        // if the parent's directionality is wrong...
        // note: the `None` case is hit if we are the root client
        if parent_pane_dir.is_none() || parent_pane_dir.unwrap() != target_directionality {
            // insert a pane above the client with the right directionality
            parent_key = self
                .insert_pane_above(focused_client_key, target_directionality)
                .unwrap();
        }

        // create the rect
        let new_rect_key = self.rects.insert(Rectangle {
            parent: parent_key,
            // this default will be overriden by the `update_rectangle` down below
            cached_dimensions: Dimensions::default(),
            contents: RectangleContents::Client(Client { frame }),
        });

        // get the parent of the focused client (may have been modified above)
        let parent_pane = self.rects.get_mut(parent_key).unwrap().unwrap_pane_mut();

        // get the index we want to insert at
        let mut index = parent_pane
            .children
            .iter()
            .copied()
            .position(|v| v == focused_client_key)
            .unwrap();

        if let Down | Right = direction {
            index += 1;
        }

        // insert
        parent_pane.children.insert(index, new_rect_key);

        self.clients.insert(win, new_rect_key);

        self.focused = Some(win);

        // update
        self.update_rectangle(conn, parent_key, None).await?;

        frame.map(conn).await?;

        Ok(())
    }

    async fn add_first_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        let frame = frame(conn, win).await?;

        let root_geo = conn.default_root().geometry_immediate_async(conn).await?;

        let key = self.rects.insert_with_key(|key| Rectangle {
            parent: key,
            cached_dimensions: Dimensions {
                x: root_geo.x.try_into().unwrap(),
                y: root_geo.y.try_into().unwrap(),
                width: root_geo.width,
                height: root_geo.height,
            },
            contents: RectangleContents::Client(Client { frame }),
        });

        self.clients.insert(win, key);

        self.focused = Some(win);

        self.update_rectangle(conn, key, None).await?;

        frame.map(conn).await?;

        Ok(())
    }

    // TODO: maybe `https://crates.io/crates/async_recursion`?
    #[must_use]
    fn update_rectangle<'a, Dpy: AsyncDisplay + ?Sized>(
        &'a mut self,
        conn: &'a mut Dpy,
        key: XcrabKey,
        dimensions: Option<Dimensions>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(async move {
            let rect = self
                .rects
                .get_mut(key)
                .ok_or(XcrabError::ClientDoesntExist)?;

            let dimensions = dimensions.unwrap_or(rect.cached_dimensions);
            rect.cached_dimensions = dimensions;

            match &mut rect.contents {
                RectangleContents::Pane(pane) => {
                    // TODO: gap

                    let new_dimensions = dimensions.split(pane.directionality, pane.children.len());

                    for (key, dimensions) in pane
                        .children
                        .clone()
                        .into_iter()
                        .zip(new_dimensions.into_iter())
                    {
                        self.update_rectangle(conn, key, Some(dimensions)).await?;
                    }
                }
                RectangleContents::Client(client) => {
                    client
                        .frame
                        .configure(
                            conn,
                            ConfigureWindowParameters {
                                x: Some(dimensions.x.into()),
                                y: Some(dimensions.y.into()),
                                width: Some(dimensions.width.into()),
                                height: Some(dimensions.height.into()),
                                ..Default::default()
                            },
                            self.focused.unwrap(),
                        )
                        .await?;
                }
            }

            Ok(())
        })
    }

    pub fn has_client(&self, win: Window) -> bool {
        self.clients.contains_key(&win)
    }

    pub async fn remove_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        let client_key = *self
            .clients
            .get(&win)
            .ok_or(XcrabError::ClientDoesntExist)?;

        let client = self.rects.get(client_key).unwrap();

        client.unwrap_client().frame.unframe(conn).await?;

        let parent_key = client.parent;
        let parent = self.rects.get_mut(parent_key).unwrap();

        parent
            .unwrap_pane_mut()
            .children
            .retain(|&v| v != client_key);

        self.clients.remove(&win);
        self.rects.remove(client_key);

        if self.focused.unwrap() == win {
            self.focused = Some(*self.clients.keys().next().unwrap());
        }

        self.update_rectangle(conn, parent_key, None).await?;

        Ok(())
    }
}

pub fn may_not_exist(res: breadx::Result) -> breadx::Result {
    match res {
        // if its a `Window` error, that means it happened because
        // a window failed to exist, and we want to allow those
        Err(BreadError::XProtocol {
            error_code: ErrorCode(3),
            ..
        }) => Ok(()),
        v => v,
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
        focused_win: Window,
    ) -> Result<()> {
        let border_size = CONFIG.border_size();
        let gap_size = CONFIG.gap_size();

        let coordinate_inset = i32::from(gap_size);
        let dimension_inset = 2 * (u32::from(gap_size) + u32::from(border_size));

        let width = props.width.map(|v| v - dimension_inset);
        let height = props.height.map(|v| v - dimension_inset);

        let focused = focused_win == self.win;

        self.frame
            .change_attributes_async(
                conn,
                WindowParameters {
                    border_pixel: Some(if focused {
                        CONFIG.focused_color()
                    } else {
                        CONFIG.border_color()
                    }),
                    ..Default::default()
                },
            )
            .await?;

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

        may_not_exist(
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
                .await,
        )?;

        Ok(())
    }

    async fn map<Dpy: AsyncDisplay + ?Sized>(self, conn: &mut Dpy) -> Result<()> {
        may_not_exist(self.win.map_async(conn).await)?;
        self.frame.map_async(conn).await?;

        Ok(())
    }

    async fn unframe<Dpy: AsyncDisplay + ?Sized>(self, conn: &mut Dpy) -> Result<()> {
        let root = conn.default_root();

        self.frame.unmap_async(conn).await?;

        may_not_exist(self.win.reparent_async(conn, root, 0, 0).await)?;
        // no longer related to us, remove from save set
        may_not_exist(self.win.change_save_set_async(conn, SetMode::Delete).await)?;

        self.frame.free_async(conn).await?;

        Ok(())
    }
}

async fn frame<Dpy: AsyncDisplay + ?Sized>(conn: &mut Dpy, win: Window) -> Result<FramedWindow> {
    let root = conn.default_root();

    // here, we cant use `may_not_exist` because we need the geometry
    let geometry = win.geometry_immediate_async(conn).await?;

    let frame = conn
        .create_simple_window_async(
            root,
            // theoretically, all of these could be ignoring since they are set later
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

    may_not_exist(win.change_save_set_async(conn, SetMode::Insert).await)?;

    may_not_exist(win.reparent_async(conn, frame, 0, 0).await)?;

    Ok(FramedWindow { frame, win })
}
