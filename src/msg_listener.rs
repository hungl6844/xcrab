use crate::Result;
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;

macro_rules! unwrap_or_continue {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => continue,
        }
    };
}

// TODO: Accept some sort of handle to perform tasks on the WM
pub async fn listener_task(socket_path: &Path) -> Result<()> {
    let listener = UnixListener::bind(socket_path)?;
    loop {
        let (mut stream, _) = unwrap_or_continue!(listener.accept().await);
        let mut buf = String::new();

        stream.read_to_string(&mut buf).await?;

        println!("{}", buf);
    }
}
