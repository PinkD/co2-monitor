#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
extern crate alloc;

use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    main,
    spi::master::{Config as SpiConfig, Spi},
    time,
};
use esp_wifi::wifi;

use blocking_network_stack::{IoError, Stack, UdpSocket};
use embedded_graphics::image::Image;
use embedded_graphics::pixelcolor::Gray4;
use embedded_graphics::prelude::*;
use smoltcp::{iface, socket, wire};
use tinybmp::Bmp;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    warn!("{}", info);
    loop {}
}

use esp_println::logger;

esp_bootloader_esp_idf::esp_app_desc!();

use co2_monitor::canvas::{Canvas, Screen};
use co2_monitor::e_paper::EPaper;
use co2_monitor::scd41::{MeasureResult, SCD41};
use co2_monitor::utils::debug_alloc;
use co2_monitor::{config, net};
use log::{debug, info, warn};

#[main]
fn main() -> ! {
    // logger::init_logger(log::LevelFilter::Trace);
    logger::init_logger(log::LevelFilter::Info);
    esp_alloc::heap_allocator!(size: 128 * 1024);
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let time_group = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);
    let rand = rng.random();
    let wifi_controller = esp_wifi::init(time_group.timer0, rng).unwrap();
    let (mut controller, interfaces) = wifi::new(&wifi_controller, peripherals.WIFI).unwrap();
    let device = interfaces.sta;

    let mut socket_set_entries: [iface::SocketStorage; 3] = Default::default();
    let mut ss = iface::SocketSet::new(&mut socket_set_entries[..]);
    let mut dhcp_socket = socket::dhcpv4::Socket::new();
    // set a hostname
    dhcp_socket.set_outgoing_options(&[wire::DhcpOption {
        kind: 12,
        data: b"esp-wifi",
    }]);
    ss.add(dhcp_socket);
    let stack = create_network_stack(&mut controller, device, ss, rand);

    let mut sb = net::SocketBuff::new();
    let mut socket = stack.get_udp_socket(
        sb.rx_meta.as_mut_slice(),
        &mut sb.rx_buffer,
        sb.tx_meta.as_mut_slice(),
        &mut sb.tx_buffer,
    );
    socket.bind(config::METRIC_PORT).unwrap();

    // led test
    // let mut led = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());
    // led.set_high();
    let delay = Delay::new();
    // delay.delay_millis(1000);

    let power = Output::new(peripherals.GPIO15, Level::Low, OutputConfig::default());
    let busy = Input::new(peripherals.GPIO2, InputConfig::default());
    let reset = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let (cs, clk, din) = (peripherals.GPIO17, peripherals.GPIO5, peripherals.GPIO18);
    let spi = Spi::new(peripherals.SPI2, SpiConfig::default())
        .unwrap()
        .with_cs(cs)
        .with_sck(clk)
        .with_mosi(din);

    let size = Size::new(296, 128);
    let ep = EPaper::new(&size, spi, power, busy, reset, dc);
    ep.init_black_white().unwrap();
    info!("init finish");
    delay.delay_millis(1000);

    // power up scd sensor
    let mut scd_power = Output::new(peripherals.GPIO23, Level::High, OutputConfig::default());
    scd_power.set_high();
    let i2c = I2c::new(peripherals.I2C0, I2cConfig::default())
        .unwrap()
        .with_scl(peripherals.GPIO22)
        .with_sda(peripherals.GPIO21);
    let mut scd = SCD41::new(i2c);
    info!("scd init");
    // NOTE: adjust temperature offset, default is 4.0
    // scd_setting(&scd, 1.5);

    if let Err(err) = scd.start_low_power() {
        panic!("error: {:?}", err);
    }
    let delay = Delay::new();
    info!("scd start");
    // delay.delay_millis(1000);

    let mut screen = Screen::new(&size);
    let mut count = 1;
    let full_screen_update_count = 100;
    let mut last_measure = Default::default();
    loop {
        info!("scd measure");
        // match scd.measure_oneshot() {
        match scd.measure() {
            Ok(m) => {
                if last_measure == m {
                    info!("not change");
                    delay.delay_millis(10000);
                    continue;
                }
                info!("co2: {}, temp: {}, hum: {}", m.co2_ppm, m.temp, m.hum);
                match send_metric(&mut socket, &m) {
                    Ok(_) => {}
                    Err(err) => {
                        warn!("failed to send metric: {:?}", err);
                    }
                }
                // NOTE: show memory alloc before and after render canvas
                debug_alloc("before render");
                let data = screen.render(&m);
                debug_alloc("render");
                debug!("data len: {}", data.len());
                if count % full_screen_update_count == 0 {
                    ep.init_black_white().unwrap();
                    ep.display_black_white(data.as_slice()).unwrap();
                    debug_alloc("after display");
                } else {
                    ep.display_partial(data.as_slice()).unwrap();
                    debug_alloc("after display partial");
                }
                info!("display finish");
                ep.halt().unwrap();
                last_measure = m;
            }
            Err(err) => {
                warn!("error: {:?}", err);
            }
        }

        info!("updated, count: {}", count);
        count += 1;
        delay.delay_millis(10000);
    }
}

#[allow(dead_code)]
fn _backup_for_img_display() -> ! {
    esp_alloc::heap_allocator!(size: 128 * 1024);
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let delay = Delay::new();
    delay.delay_millis(1000);

    let power = Output::new(peripherals.GPIO15, Level::Low, OutputConfig::default());
    let busy = Input::new(peripherals.GPIO2, InputConfig::default());
    let reset = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let (cs, clk, din) = (peripherals.GPIO17, peripherals.GPIO5, peripherals.GPIO18);
    let spi = Spi::new(peripherals.SPI2, SpiConfig::default())
        .unwrap()
        .with_cs(cs)
        .with_sck(clk)
        .with_mosi(din);

    let size = Size::new(296, 128);
    let ep = EPaper::new(&size, spi, power, busy, reset, dc);
    ep.init_gray4().unwrap();
    info!("init finish");
    delay.delay_millis(1000);
    // let len = size.width / 8 * size.height * 2;
    // 0b00100111, 00 10 01 11, white gray1 gray2 black
    // fill white
    // let data = vec![0xaa; len as usize];
    // fill gradient, aka 0b00100111
    // let data = vec![0x27; len as usize];
    let mut canvas = Canvas::new(&size);
    let bmp = load_img();
    // debug_alloc("load img");
    let x_start = ((size.width - bmp.size().width) / 2) as i32;
    let y_start = ((size.height - bmp.size().height) / 2) as i32;
    let point = Point::new(x_start, y_start);
    info!("image start point: {:?}", point);
    // force drop img
    let img = Image::new(&bmp, point);
    info!("image size: {:?}", img.bounding_box());
    info!("pre draw to canvas");
    debug_alloc("new img");
    img.draw(&mut canvas).unwrap();
    debug_alloc("draw canvas");
    info!("draw to canvas");
    let data = canvas.render_gray();
    debug_alloc("render");
    info!("data len: {}", data.len());
    ep.display_gray4(data.as_slice()).unwrap();
    debug_alloc("display");
    ep.halt().unwrap();
    info!("display finish");
    delay.delay_millis(1000);

    loop {
        delay.delay_millis(5000);
    }
}

#[allow(dead_code)]
fn load_img<'a>() -> Bmp<'a, Gray4> {
    // let bmp_data = include_bytes!("../../assets/kujo.bmp");
    let bmp_data: &'a [u8] = [1].as_slice();

    info!("pre load bmp image");
    // let bmp = Bmp::from_slice(bmp_data.as_slice()).unwrap();
    let bmp = Bmp::from_slice(&bmp_data).unwrap();
    info!("load bmp image");

    let size = bmp.size();
    info!("bmp size: {:?}", size);
    bmp
}

#[allow(dead_code)]
fn scd_setting(scd: &SCD41, offset: f32) {
    match scd.get_temperature_offset() {
        Ok(offset) => {
            info!("temp offset {}", offset);
        }
        Err(err) => {
            warn!("get temp offset error: {:?}", err);
        }
    }
    match scd.set_temperature_offset(offset) {
        Ok(_) => {
            info!("temp offset set");
        }
        Err(err) => {
            warn!("set temp offset error: {:?}", err);
        }
    }
    match scd.persist_settings() {
        Ok(_) => {
            info!("persist settings");
        }
        Err(err) => {
            warn!("persist settings error: {:?}", err);
        }
    }
}

// cannot put this in net.rs because of lifetime problem
pub fn create_network_stack<'a>(
    controller: &mut wifi::WifiController,
    mut device: wifi::WifiDevice<'a>,
    ss: iface::SocketSet<'a>,
    rand: u32,
) -> Stack<'a, wifi::WifiDevice<'a>> {
    let interface = iface::Interface::new(
        iface::Config::new(wire::HardwareAddress::Ethernet(
            wire::EthernetAddress::from_bytes(&device.mac_address()),
        )),
        &mut device,
        // now
        smoltcp::time::Instant::from_micros(
            time::Instant::now().duration_since_epoch().as_micros() as i64,
        ),
    );

    controller
        .set_power_saving(esp_wifi::config::PowerSaveMode::None)
        .unwrap();

    let now = || time::Instant::now().duration_since_epoch().as_millis();
    let stack = Stack::new(interface, device, ss, now, rand);

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
    info!("connect to wifi");

    let delay = Delay::new();
    // wait to get connected
    debug!("wait to get connected");
    let mut wait_count = 10;
    loop {
        if controller.is_connected().unwrap() {
            info!("wifi connected");
            break;
        } else {
            info!("wifi not connected, wait 1s, wait count {}", wait_count);
        }
        delay.delay_millis(1000);
        wait_count -= 1;
        if wait_count == 0 {
            warn!("wait wifi connected timeout");
            break;
        }
    }
    info!("setting dhcp");

    let mut count = 0;
    loop {
        // let stack make progress
        stack.work();

        if stack.is_iface_up() {
            debug!("stack ready, ip info {:?}", stack.get_ip_info().unwrap());
            break;
        }
        debug!("stack not ready, wait 1s, count {}", count);
        count += 1;
        if count > 10 {
            warn!("wait stack ready timeout");
            break;
        }
        delay.delay_millis(1000);
    }
    stack
}

pub fn send_metric(
    socket: &mut UdpSocket<wifi::WifiDevice>,
    m: &MeasureResult,
) -> Result<(), IoError> {
    let addr = blocking_network_stack::ipv4::Ipv4Addr::from(net::parse_ip(config::METRIC_SERVER));
    let port = config::METRIC_PORT;
    debug!("start to send metric to udp://{}:{}", addr, port);
    let data = [
        m.temp.to_be_bytes().as_slice(),
        m.hum.to_be_bytes().as_slice(),
        m.co2_ppm.to_be_bytes().as_slice(),
    ]
    .concat();
    socket.send(addr.into(), port, data.as_slice())
}
