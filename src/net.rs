use crate::scd41::MeasureResult;
use crate::{config, info, warn};
use crate::{debug, log};
use alloc::format;
use blocking_network_stack::Stack;
use embedded_io::*;
use esp_hal::time;
use esp_println::println;
use esp_wifi::wifi;

fn timestamp() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(
        esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_micros() as i64,
    )
}

fn create_interface(device: &mut wifi::WifiDevice) -> smoltcp::iface::Interface {
    // users could create multiple instances but since they only have one WifiDevice
    // they probably can't do anything bad with that
    smoltcp::iface::Interface::new(
        smoltcp::iface::Config::new(smoltcp::wire::HardwareAddress::Ethernet(
            smoltcp::wire::EthernetAddress::from_bytes(&device.mac_address()),
        )),
        device,
        timestamp(),
    )
}

fn parse_ip(ip: &str) -> [u8; 4] {
    let mut result = [0u8; 4];
    for (idx, octet) in ip.split(".").into_iter().enumerate() {
        result[idx] = u8::from_str_radix(octet, 10).unwrap();
    }
    result
}

pub fn run_net(mut controller: wifi::WifiController, mut device: wifi::WifiDevice, rand: u32) {
    let interface = create_interface(&mut device);

    controller
        .set_power_saving(esp_wifi::config::PowerSaveMode::None)
        .unwrap();

    let mut socket_set_entries: [smoltcp::iface::SocketStorage; 3] = Default::default();
    let socket_set = smoltcp::iface::SocketSet::new(&mut socket_set_entries[..]);

    let now = || time::Instant::now().duration_since_epoch().as_millis();
    let mut stack = Stack::new(interface, device, socket_set, now, rand);

    let client_config = wifi::Configuration::Client(wifi::ClientConfiguration {
        ssid: config::SSID.into(),
        password: config::PASSWORD.into(),
        ..Default::default()
    });
    let res = controller.set_configuration(&client_config);
    debug!("wifi_set_configuration returned {:?}", res);

    controller.start().unwrap();
    debug!("is wifi started: {:?}", controller.is_started());

    info!("scan wifi");
    let res = controller.scan_n(10).unwrap();
    for ap in res {
        info!("{:?}", ap);
    }

    debug!("capabilities: {:?}", controller.capabilities());
    controller.connect().unwrap();
    debug!("connect to wifi");

    // wait to get connected
    debug!("wait to get connected");
    loop {
        match controller.is_connected() {
            Ok(true) => break,
            Ok(false) => {}
            Err(err) => {
                warn!("wifi.is_connected: {:?}", err);
                loop {}
            }
        }
    }
    info!("wifi connected");

    info!("setting ip {}", config::STATIC_IP);

    stack
        .set_iface_configuration(&blocking_network_stack::ipv4::Configuration::Client(
            blocking_network_stack::ipv4::ClientConfiguration::Fixed(
                blocking_network_stack::ipv4::ClientSettings {
                    ip: blocking_network_stack::ipv4::Ipv4Addr::from(parse_ip(config::STATIC_IP)),
                    subnet: blocking_network_stack::ipv4::Subnet {
                        gateway: blocking_network_stack::ipv4::Ipv4Addr::from(parse_ip(
                            config::GATEWAY_IP,
                        )),
                        mask: blocking_network_stack::ipv4::Mask(24),
                    },
                    dns: None,
                    secondary_dns: None,
                },
            ),
        ))
        .unwrap();

    info!("start listen on http://{}:8080/", config::STATIC_IP);

    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    let mut socket = stack.get_socket(&mut rx_buffer, &mut tx_buffer);

    socket.listen(8080).unwrap();
    loop {
        socket.work();

        if !socket.is_open() {
            socket.listen(8080).unwrap();
        }

        // TODO: this is copied from official wifi example, need to update
        if socket.is_connected() {
            debug!("socket connected");

            let mut time_out = false;
            let deadline = time::Instant::now() + time::Duration::from_secs(10);
            let mut buffer = [0u8; 1024];
            let mut pos = 0;
            while let Ok(len) = socket.read(&mut buffer[pos..]) {
                let to_print = unsafe { str::from_utf8_unchecked(&buffer[..(pos + len)]) };

                if to_print.contains("\r\n\r\n") {
                    debug!("{}", to_print);
                    break;
                }

                pos += len;

                if time::Instant::now() > deadline {
                    debug!("timeout");
                    time_out = true;
                    break;
                }
            }

            if !time_out {
                let m: MeasureResult = Default::default();
                let header = "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\n\r\n";
                let body = format!(
                    r#"
# HELP temperature in celsius
# TYPE temperature gauge
temperature{{}} {}
# HELP humidity in percentage
# TYPE humidity gauge
humidity{{}} {}
# HELP co2 ppm
# TYPE co2_ppm gauge
co2_ppm{{}} {}
"#,
                    m.temp, m.hum, m.co2_ppm
                );

                socket.write_all(body.as_bytes()).unwrap();

                socket.flush().unwrap();
            }
            socket.close();
        }

        let deadline = time::Instant::now() + time::Duration::from_secs(5);
        while time::Instant::now() < deadline {
            socket.work();
        }
    }
}
