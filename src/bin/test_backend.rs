use std::process;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let backend = match cosmic_connect::backend::KdeConnectBackend::new().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ERROR: Failed to connect to D-Bus session bus: {e}");
            eprintln!();
            eprintln!("Make sure the KDE Connect daemon is running:");
            eprintln!("  systemctl --user start kdeconnect");
            process::exit(1);
        }
    };

    let devices = backend.devices().await;

    if devices.is_empty() {
        println!("No paired devices found.");
        println!("Pair a device using kdeconnect-cli or the KDE Connect app.");
        process::exit(0);
    }

    println!("Found {} device(s):\n", devices.len());

    for device in &devices {
        println!("  Device: {}", device.name);
        println!("    ID:     {}", device.id);
        println!("    Type:   {:?}", device.device_type);
        println!("    Reach:  {}", if device.is_reachable { "yes" } else { "no" });
        println!("    Paired: {}", if device.is_paired { "yes" } else { "no" });
        println!("    State:  {} (0=not, 1=req, 2=peer, 3=paired)", device.pair_state);
        println!("    Count:  {} plugin(s)", device.supported_plugins.len());

        if let Some(bat) = &device.battery {
            println!("    Bat:    {}% {}",
                bat.charge,
                if bat.is_charging { "(charging)" } else { "" },
            );
        } else {
            println!("    Bat:    N/A (offline or no battery plugin)");
        }

        if device.is_reachable {
            if device.has_plugin("kdeconnect_findmyphone") {
                println!("    Ring:   available");
            }
            if device.has_plugin("kdeconnect_ping") {
                println!("    Ping:   available");
            }
            if device.has_plugin("kdeconnect_clipboard") {
                println!("    Clip:   available");
            }
            if device.has_plugin("kdeconnect_share") {
                println!("    Share:  available");
            }
            if device.has_plugin("kdeconnect_sms") {
                println!("    SMS:    available");
                backend.request_all_conversations(&device.id).await;
                println!("    (requested conversations, waiting 4s…)");
                tokio::time::sleep(Duration::from_secs(4)).await;
                let convos = backend.active_conversations(&device.id).await;
                println!("    Conversations: {}", convos.len());
                for c in &convos {
                    let preview = if c.body.len() > 40 {
                        format!("{}…", &c.body[..40])
                    } else {
                        c.body.clone()
                    };
                    println!("      thread={} from={}: {} (type={})",
                        c.thread_id,
                        c.addresses.first().map(|a| a.address.as_str()).unwrap_or("?"),
                        preview, c.message_type);
                }
            }
        }

        println!();
    }
}
