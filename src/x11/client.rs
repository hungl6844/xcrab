use breadx::prelude::{AsyncDisplayXprotoExt, SetMode};
use breadx::{AsyncDisplay, ConfigureWindowParameters, Window};
use std::collections::HashMap;

pub struct XcrabClient {
    geometry: XcrabGeometry,
    position: usize,
    pub parent: Window,
}

#[derive(Debug)]
pub struct XcrabGeometry {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

impl XcrabClient {
    pub async fn new<Dpy: AsyncDisplay + ?Sized>(
        window: Window,
        dpy: &mut Dpy,
        position: usize,
    ) -> Result<Self, crate::XcrabError> {
        let geometry = window.geometry_immediate_async(dpy).await?;

        let root = dpy.default_root();
        let parent = dpy
            .create_simple_window_async(
                root,
                geometry.x,
                geometry.y,
                geometry.width,
                geometry.height,
                crate::BORDER_WIDTH,
                0xff_00_00,
                0x00_00_00,
            )
            .await?;

        Ok(XcrabClient {
            geometry: XcrabGeometry {
                x: geometry.x,
                y: geometry.y,
                width: geometry.width,
                height: geometry.height,
            },
            position,
            parent,
        })
    }

    pub fn update_geometry(&mut self, geometry: XcrabGeometry) {
        self.geometry = geometry;
    }
}

pub async fn calculate_geometry<Dpy: AsyncDisplay + ?Sized>(
    windows: &mut HashMap<Window, XcrabClient>,
    dpy: &mut Dpy,
) -> Result<(), crate::XcrabError> {
    let root = dpy.default_root();
    let root_geometry = root.geometry_immediate_async(dpy).await?;
    let window_count = windows.len();

    // let gap_width = crate::GAP_WIDTH as usize * (window_count + 1);
    let width_per_window = root_geometry.width as usize / window_count;
    let border_width = crate::BORDER_WIDTH as usize * 2;

    for (window, xcrab_client) in windows {
        let xcrab_geometry = XcrabGeometry {
            x: xcrab_client.position as i16 * width_per_window as i16,
            y: xcrab_client.geometry.y,
            width: (width_per_window - border_width) as u16,
            height: xcrab_client.geometry.height,
        };

        xcrab_client.update_geometry(xcrab_geometry);

        window.change_save_set_async(dpy, SetMode::Insert).await?;
        xcrab_client
            .parent
            .configure_async(
                dpy,
                ConfigureWindowParameters {
                    x: Some(xcrab_client.geometry.x.into()),
                    y: Some(xcrab_client.geometry.y.into()),
                    width: Some(xcrab_client.geometry.width.into()),
                    height: Some(xcrab_client.geometry.height.into()),
                    ..breadx::ConfigureWindowParameters::default()
                },
            )
            .await?;

        window
            .configure_async(
                dpy,
                ConfigureWindowParameters {
                    width: Some(xcrab_client.geometry.width.into()),
                    height: Some(xcrab_client.geometry.height.into()),
                    ..breadx::ConfigureWindowParameters::default()
                },
            )
            .await?;

        window
            .reparent_async(dpy, xcrab_client.parent, 0, 0)
            .await?;

        xcrab_client.parent.map_async(dpy).await?;

        window.map_async(dpy).await?;

        println!("{:#?}", xcrab_client.geometry);
    }

    Ok(())
}
