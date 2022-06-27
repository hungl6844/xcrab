use breadx::prelude::{AsyncDisplayXprotoExt, SetMode};
use breadx::{AsyncDisplay, BreadError, ConfigureWindowParameters, ErrorCode, EventMask, Window};
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
            use {Direction::*, Directionality::*};

            // this code path is somewhat difficult to understand, so i added some comments

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
                    // uh oh, we hit the top, time to create a pane

                    child_key = focused_client_key;
                    let child = self.rects.get(child_key).unwrap();
                    // parent of the focused client
                    let parent_key = child.parent;

                    // the new pane
                    let new_pane = Rectangle {
                        parent: parent_key,
                        cached_dimensions: child.cached_dimensions,
                        contents: RectangleContents::Pane(Pane {
                            // its child is the focused client
                            children: vec![child_key],
                            directionality: target_directionality,
                        }),
                    };

                    let new_pane_key = self.rects.insert(new_pane);

                    // create the new_pane -> child relationship
                    self.rects.get_mut(child_key).unwrap().parent = new_pane_key;

                    let parent = self.rects.get_mut(parent_key).unwrap();
                    match &mut parent.contents {
                        RectangleContents::Pane(pane) => {
                            let index = pane
                                .children
                                .iter()
                                .copied()
                                .position(|v| v == child_key)
                                .unwrap();
                            // remove the parent -> child relation and replace it with parent -> new_pane
                            pane.children[index] = new_pane_key;
                        }
                        // this means that the focused client is the root client
                        RectangleContents::Client(_) => {}
                    }

                    break new_pane_key;
                }

                let parent = self.rects.get(parent_key).unwrap();

                match &parent.contents {
                    RectangleContents::Pane(pane) => {
                        if pane.directionality == target_directionality {
                            // yay! found it
                            break parent_key;
                        }

                        // nope, continue
                        child_key = parent_key;
                    }
                    // parents should never be clients, only panes
                    RectangleContents::Client(_) => unreachable!(),
                }
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
            let parent = match &mut self.rects.get_mut(parent_key).unwrap().contents {
                RectangleContents::Pane(pane) => pane,
                RectangleContents::Client(_) => unreachable!(),
            };

            // the index which we want to `insert` at, found using `child_key`
            let mut index = parent
                .children
                .iter()
                .copied()
                .position(|v| v == child_key)
                .unwrap();

            if let Down | Right = direction {
                index += 1;
            }

            // insert the new rect
            parent.children.insert(index, new_rect_key);

            self.clients.insert(win, new_rect_key);

            self.focused = Some(win);

            // update the parent rectangle to also update all the siblings of out new rect
            self.update_rectangle(conn, parent_key, None).await?;
        } else {
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
        }

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

        match client.contents {
            RectangleContents::Pane(_) => unreachable!(),
            RectangleContents::Client(client) => {
                client.frame.unframe(conn).await?;
            }
        }

        let parent_key = client.parent;
        let parent = self.rects.get_mut(parent_key).unwrap();

        match &mut parent.contents {
            RectangleContents::Pane(pane) => {
                pane.children.retain(|&v| v != client_key);
            }
            RectangleContents::Client(_) => unreachable!(),
        }

        self.update_rectangle(conn, parent_key, None).await?;

        self.clients.remove(&win);
        self.rects.remove(client_key);

        if self.focused.unwrap() == win {
            self.focused = Some(*self.clients.keys().next().unwrap());
        }

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
