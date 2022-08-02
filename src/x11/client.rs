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

use breadx::{auto::xproto::{ClientMessageEvent, InputFocus, SetInputFocusRequest}, client_message_data::ClientMessageData, prelude::{AsByteSequence, AsyncDisplayXprotoExt, PropertyType, SetMode}, AsyncDisplay, AsyncDisplayExt, Atom, BreadError, ConfigureWindowParameters, ErrorCode, Event, EventMask, Window, WindowParameters, XidType, KeyboardState};
use slotmap::{new_key_type, SlotMap};
use std::{collections::HashMap, future::Future, pin::Pin, slice};
use breadx::auto::xproto::{KeyButMask, Keycode, Keysym};

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
        let count_u16 = u16::try_from(count).unwrap();
        match direction {
            Directionality::Horizontal => {
                let amount_for_windows = self.width - CONFIG.gap_size() * (count_u16 - 1);
                let excess = amount_for_windows % count_u16;
                let window_size = amount_for_windows / count_u16;
                let window_stride = window_size + CONFIG.gap_size();

                (0..count.try_into().unwrap())
                    .map(|i| Dimensions {
                        x: self.x + i * window_stride + if i < excess { 1 } else { 0 },
                        width: window_size,
                        ..self
                    })
                    .collect()
            }
            Directionality::Vertical => {
                let amount_for_windows = self.height - CONFIG.gap_size() * (count_u16 - 1);
                let excess = amount_for_windows % count_u16;
                let window_size = amount_for_windows / count_u16;
                let window_stride = window_size + CONFIG.gap_size();

                (0..count.try_into().unwrap())
                    .map(|i| Dimensions {
                        y: self.y + i * window_stride + if i < excess { 1 } else { 0 },
                        height: window_size,
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

#[derive(Debug, Clone)]
struct Pane {
    children: Vec<XcrabKey>,
    directionality: Directionality,
}

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

    async fn focus_update_map<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        frame: FramedWindow,
        parent_key: XcrabKey,
    ) -> Result<()> {
        let win = frame.win;

        // we cant `set_focus` here since `win` isnt yet mapped
        self.focused = Some(win);

        self.update_rectangle(conn, parent_key, None).await?;

        frame.map(conn).await?;

        self.update_focused(conn).await?;

        Ok(())
    }

    /// Tells x about the currently focused window.
    pub async fn update_focused<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
    ) -> Result<()> {
        // unfortunately, i cannot find a method on `conn` to set the focus.

        // https://www.x.org/releases/current/doc/xproto/x11protocol.html#Encoding::Requests
        let mut req = SetInputFocusRequest {
            req_type: 42, // constant, specified in x protocol docs.
            revert_to: InputFocus::None,
            length: 3,                  // constant, specified in x protocol docs.
            focus: Window::from_xid(0), // None
            time: 0,                    // CurrentTime
        };

        if let Some(focus) = self.focused {
            req.focus = focus;
        }

        conn.exchange_request_async(req).await?;

        Ok(())
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

        self.focus_update_map(conn, frame, parent_key).await?;

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

        self.focus_update_map(conn, frame, parent_key).await?;

        Ok(())
    }

    async fn add_first_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        let frame = frame(conn, win).await?;

        let root_geo = conn.default_root().geometry_immediate_async(conn).await?;

        let outer_gap_size = CONFIG.outer_gap_size();
        let key = self.rects.insert_with_key(|key| Rectangle {
            parent: key,
            cached_dimensions: Dimensions {
                x: u16::try_from(root_geo.x).unwrap() + outer_gap_size,
                y: u16::try_from(root_geo.y).unwrap() + outer_gap_size,
                width: root_geo.width - 2 * outer_gap_size,
                height: root_geo.height - 2 * outer_gap_size,
            },
            contents: RectangleContents::Client(Client { frame }),
        });

        self.clients.insert(win, key);

        self.focus_update_map(conn, frame, key).await?;

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
                    if !pane.children.is_empty() {
                        let new_dimensions =
                            dimensions.split(pane.directionality, pane.children.len());

                        for (key, dimensions) in pane
                            .children
                            .clone()
                            .into_iter()
                            .zip(new_dimensions.into_iter())
                        {
                            self.update_rectangle(conn, key, Some(dimensions)).await?;
                        }
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
            self.focused = self.clients.keys().copied().next();

            self.update_focused(conn).await?;
        }

        self.update_rectangle(conn, parent_key, None).await?;

        // TODO: remove panes if they have 1 or 0 children

        Ok(())
    }

    pub async fn destroy_focused_client<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
    ) -> Result<()> {
        if let Some(focused) = self.focused {
            let client_key = *self
                .clients
                .get(&focused)
                .ok_or(XcrabError::ClientDoesntExist)?;

            let frame = self.rects.get(client_key).unwrap().unwrap_client().frame;

            self.remove_client(conn, focused).await?;

            frame.kill_client(conn).await?;

            Ok(())
        } else {
            Ok(())
        }
    }

    pub async fn set_focus<Dpy: AsyncDisplay + ?Sized>(
        &mut self,
        conn: &mut Dpy,
        win: Window,
    ) -> Result<()> {
        let client_key = *self
            .clients
            .get(&win)
            .ok_or(XcrabError::ClientDoesntExist)?;

        self.focused = Some(win);

        self.update_focused(conn).await?;

        self.update_rectangle(conn, self.rects.get(client_key).unwrap().parent, None)
            .await?;

        Ok(())
    }

    pub async fn get_focused(&self) -> Option<Window> {
        self.focused
    }

    pub async fn get_framed_window(&self, window: Window) -> FramedWindow {
        let focused_key = self.clients.get(&window).unwrap();
        let focused = self.rects.get(*focused_key).unwrap();
        let focused_frame = focused.unwrap_client().frame;
        focused_frame
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
pub struct FramedWindow {
    pub frame: Window,
    pub win: Window,
}

impl FramedWindow {
    async fn configure<Dpy: AsyncDisplay + ?Sized>(
        self,
        conn: &mut Dpy,
        props: ConfigureWindowParameters,
        focused_win: Window,
    ) -> Result<()> {
        let inset = 2 * u32::from(CONFIG.border_size());

        let width = props.width.map(|v| v - inset);
        let height = props.height.map(|v| v - inset);

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
                    x: props.x,
                    y: props.y,
                    width,
                    height,
                    border_width: Some(CONFIG.border_size().into()),
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

        may_not_exist(self.win.unmap_async(conn).await)?;

        may_not_exist(self.win.reparent_async(conn, root, 0, 0).await)?;
        // no longer related to us, remove from save set
        may_not_exist(self.win.change_save_set_async(conn, SetMode::Delete).await)?;

        self.frame.free_async(conn).await?;

        Ok(())
    }

    async fn kill_client<Dpy: AsyncDisplay + ?Sized>(self, conn: &mut Dpy) -> Result<()> {
        struct ListOfAtom(Vec<Atom>);

        impl AsByteSequence for ListOfAtom {
            fn size(&self) -> usize {
                unimplemented!()
            }
            fn as_bytes(&self, _: &mut [u8]) -> usize {
                unimplemented!()
            }

            fn from_bytes(mut bytes: &[u8]) -> Option<(Self, usize)> {
                let mut index = 0;
                let mut vec = Vec::new();

                while let Some((atom, index2)) = Atom::from_bytes(bytes) {
                    vec.push(atom);
                    index += index2;
                    bytes = &bytes[index2..];
                }

                Some((Self(vec), index))
            }
        }

        fn as_u8_slice(data: &[u32]) -> &[u8] {
            // SAFETY: i believe in you to see that this is sound
            unsafe {
                slice::from_raw_parts(
                    data.as_ptr().cast::<u8>(),
                    data.len().checked_mul(4).unwrap(),
                )
            }
        }

        let wm_protocols = conn
            .intern_atom_immediate_async("WM_PROTOCOLS", true)
            .await?;
        assert!(wm_protocols.xid != 0);
        let wm_delete_window = conn
            .intern_atom_immediate_async("WM_DELETE_WINDOW", true)
            .await?;
        assert!(wm_delete_window.xid != 0);

        let prop = self
            .win
            .get_property_immediate_async::<_, ListOfAtom>(
                conn,
                wm_protocols,
                PropertyType::Atom,
                false,
            )
            .await?;
        let protocols = prop.unwrap().0; // should never fail to parse

        if protocols.contains(&wm_delete_window) {
            let data = [wm_delete_window.xid, 0, 0, 0, 0];
            let data_bytes = as_u8_slice(&data);

            conn.send_event_async(
                self.win,
                EventMask::default(),
                Event::ClientMessage(ClientMessageEvent {
                    event_type: 33, // constant, check x protocol docs
                    format: 32,     // tell the x server to byte-flip as if the data was a [u32]
                    sequence: 0,    // this should be filled in for us by the x server
                    window: self.win,
                    ty: wm_protocols,
                    data: ClientMessageData::from_bytes(data_bytes).unwrap().0, // why the field is private is beyond me
                }),
            )
            .await?;

            // tokio::spawn(async {
            //     tokio::time::sleep(Duration::from_secs(3)).await;

            //     // TODO: if the client isnt responding, `free_async` the window (maybe show a popup?)
            // });
        } else {
            self.win.free_async(conn).await?;
        }

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
            // theoretically, all of these could be ignored since they are set later
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

    win.set_event_mask_async(conn, EventMask::BUTTON_PRESS)
        .await?;

    may_not_exist(win.change_save_set_async(conn, SetMode::Insert).await)?;

    may_not_exist(win.reparent_async(conn, frame, 0, 0).await)?;

    Ok(FramedWindow { frame, win })
}

pub fn keymap(state: &mut KeyboardState) -> HashMap<Keysym, Keycode> {
    let mut map: HashMap<Keysym, Keycode> = HashMap::new();
    for keycode in 8..255_u8 {
        let key = state.process_keycode(keycode, KeyButMask::default());
        let keysyms = state.lookup_keysyms(keycode);
        if key != None {
            for keysym in keysyms {
                map.insert(*keysym, keycode);
            }
        }
    }
    map
}