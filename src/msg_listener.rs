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

use crate::x11::client::XcrabWindowManager;
use crate::Result;
use breadx::AsyncDisplay;
use std::path::Path;
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

macro_rules! unwrap_or_continue {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => continue,
        }
    };
}

// TODO: Accept some sort of handle to perform tasks on the WM
pub async fn listener_task<P: AsRef<Path>>(
    socket_path: P,
    sender: UnboundedSender<String>,
    mut result_recv: UnboundedReceiver<Result<()>>,
) -> Result<()> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    loop {
        let (mut stream, _) = unwrap_or_continue!(listener.accept().await);
        let mut buf = String::new();

        stream.read_to_string(&mut buf).await?;

        drop(sender.send(buf)); // go back to ms word clippy

        // we can unwrap here because if the channel is closed then something's not right
        if let Err(e) = result_recv.recv().await.unwrap() {
            stream.write_all(format!("{}", e).as_bytes()).await?;
        } else {
            stream.write_all(&[]).await?;
        }
    }
}

pub async fn on_recv<Dpy: AsyncDisplay + ?Sized>(
    data: String,
    manager: &mut XcrabWindowManager,
    conn: &mut Dpy,
    result_sender: &UnboundedSender<Result<()>>,
) -> Result<()> {
    let res = { data.parse::<Action>() };

    if let Ok(ref a) = res {
        a.eval(manager, conn).await?; // Don't send these errors over the channel, because they're
                                      // xcrab errors, not msg errors
    }

    drop(result_sender.send(res.map(|_| ())));

    Ok(())
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Action {
    Close,
}

impl FromStr for Action {
    type Err = crate::XcrabError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        #[allow(clippy::enum_glob_use)]
        use Action::*;
        let parts: Vec<String> = s
            .split(' ')
            .map(str::to_ascii_lowercase)
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return Err(String::from("No action provided").into());
        }

        macro_rules! eq_ignore_ascii_case_match {
            (($scrutinee:expr) { $($s:literal => $v:expr,)+ else => $else:expr $(,)? }) => {
                $(
                    if $scrutinee.eq_ignore_ascii_case($s) {
                        $v
                    } else
                )+ {
                    $else
                }
            };
        }

        // TODO: When more actions are added (such as focus etc), they will take arguments. In that
        // case, they will get passed the rest of `parts`.
        eq_ignore_ascii_case_match!((parts[0]) {
            "close" => Ok(Close),
            else => Err(format!("Unknown action: {}", s).into()),
        })
    }
}

impl Action {
    pub async fn eval<Dpy: AsyncDisplay + ?Sized>(
        &self,
        manager: &mut XcrabWindowManager,
        conn: &mut Dpy,
    ) -> Result<()> {
        #[allow(clippy::enum_glob_use)]
        use Action::*;

        match self {
            Close => manager.destroy_focused_client(conn).await?,
        }

        Ok(())
    }
}
