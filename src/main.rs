use breadx::{
    prelude::{AsyncDisplayXprotoExt, SetMode},
    traits::DisplayBase,
    AsyncDisplayConnection, AsyncDisplayExt, BreadError, Event, EventMask, WindowClass,
};

#[derive(Debug)] // TODO: actually print good errors on failure
enum XcrabError {
    Bread(BreadError),
}

impl From<BreadError> for XcrabError {
    fn from(v: BreadError) -> Self {
        Self::Bread(v)
    }
}

#[tokio::main]
async fn main() -> Result<(), XcrabError> {
    // connect to the x server
    let mut conn = AsyncDisplayConnection::create_async(None, None).await?;

    let root = conn.default_root();

    let substructure_redirect = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;

    // listen for substructure redirects to intercept events like window creation
    root.set_event_mask_async(&mut conn, substructure_redirect)
        .await?;

    loop {
        let ev = conn.wait_for_event_async().await?;

        match ev {
            Event::MapRequest(ev) => {
                // the client wishes for their window to be displayed. we must create a new
                // window with a titlebar and reparent the old window to this new window.

                let win = ev.window;

                let geometry = win.geometry_immediate_async(&mut conn).await?;

                let parent = conn
                    .create_simple_window_async(
                        root,
                        geometry.x,
                        geometry.y,
                        geometry.width,
                        geometry.height,
                        3,
                        0xff0000,
                        0x000000,
                    )
                    .await?;

                parent
                    .set_event_mask_async(&mut conn, substructure_redirect)
                    .await?;

                win.change_save_set_async(&mut conn, SetMode::Insert)
                    .await?;

                win.reparent_async(&mut conn, parent, 0, 0).await?;

                parent.map_async(&mut conn).await?;
            }
            _ => {}
        }
    }

    Ok(())
}
