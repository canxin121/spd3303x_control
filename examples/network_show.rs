use std::time::Duration;

use anyhow::Result;
use spd3303x_control::instrument::Spd3303x;
use tokio::time::timeout;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let host = args.get(1).map(String::as_str).unwrap_or("192.168.0.232");
    let resource = args.get(2).map(String::as_str).unwrap_or("inst0");

    let mut inst = match timeout(Duration::from_secs(5), Spd3303x::connect(host, resource)).await {
        Ok(Ok(client)) => client,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            eprintln!("连接 SPD3303X 超时（5 秒），请检查设备电源和网络连接。");
            return Ok(());
        }
    };

    let cfg = inst.network_config().await?;

    println!("IP      : {}", cfg.ip.trim());
    println!("Mask    : {}", cfg.mask.trim());
    println!("Gateway : {}", cfg.gateway.trim());
    println!("DHCP    : {}", if cfg.dhcp { "ON" } else { "OFF" });

    // 读取型示例：最后做一次软复位，保证用完后设备处于安全状态。
    inst.soft_reset().await?;
    inst.close().await?;
    Ok(())
}
