use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::error::ShadowsocksError;
use crate::method::CipherMethod;
use crate::tcp::shadowsocks_accept;

/// Run a Shadowsocks server that relays traffic to a target.
///
/// This is a test helper - not suitable for production use.
/// Compatible with `shadowsocks_connect` from `crate::tcp`.
pub async fn run_shadowsocks_server(
    listener: &tokio::net::TcpListener,
    password: &str,
    method: CipherMethod,
) -> Result<(), ShadowsocksError> {
    loop {
        let (stream, _) = listener.accept().await?;
        let password = password.to_string();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, password, method).await {
                eprintln!("client error: {}", e);
            }
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    password: String,
    method: CipherMethod,
) -> Result<(), ShadowsocksError> {
    let boxed: eggress_core::BoxStream = Box::new(stream);
    let (ss_stream, target_addr) = shadowsocks_accept(boxed, &password, method, None).await?;

    // Connect to target
    let target_str = match &target_addr.host {
        eggress_core::TargetHost::Ip(ip) => format!("{}:{}", ip, target_addr.port),
        eggress_core::TargetHost::Domain(d) => format!("{}:{}", d, target_addr.port),
    };
    let target_stream = TcpStream::connect(&target_str).await?;

    let (mut ss_read, mut ss_write) = tokio::io::split(ss_stream);
    let (mut target_read, mut target_write) = target_stream.into_split();

    let client_to_target = async {
        let _ = tokio::io::copy(&mut ss_read, &mut target_write).await;
        let _ = target_write.shutdown().await;
    };
    let target_to_client = async {
        let _ = tokio::io::copy(&mut target_read, &mut ss_write).await;
        let _ = ss_write.shutdown().await;
    };

    let _ = tokio::join!(client_to_target, target_to_client);
    Ok(())
}
