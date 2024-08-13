use std::result::Result::Ok;
use dht11::Dht11;
use esp_idf_hal::{
    delay::{Ets, FreeRtos}, gpio::*, i2c::*, io::{EspIOError, Write}, modem::Modem, peripherals::Peripherals, 
};

use esp_idf_sys::{self as _,};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop, http::client::EspHttpConnection, nvs::EspDefaultNvsPartition, wifi::{BlockingWifi, EspWifi}
};

use embedded_svc::{
    wifi::{Configuration, ClientConfiguration, AuthMethod},
    http::{client::Client as HttpClient, Method},
    utils::io,
};

use heapless::String;
use lcd_lcm1602_i2c::Lcd;
use log::{info, error};

use serde::*;
use serde_json;

// const LCD_ADDRESS: u8 = 0x27; // A0 = 1, A1 = 1, A2 = 0 see https://www.ardumotive.com/i2clcden.html

#[derive(Serialize, Deserialize)]
struct SensorData {
    humidity: f32,
    temperature: f32
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();    

    let peripherals = Peripherals::take().unwrap();

    let dht11_pin = PinDriver::input_output_od(peripherals.pins.gpio5.downgrade()).unwrap();
    let mut dht11 = Dht11::new(dht11_pin);
    let mut temperatures: Vec<f32> = vec![];
    let mut humidities: Vec<f32> = vec![];
    let mut median_temperature: f32;
    let mut median_humidity: f32;    

    // let i2c = peripherals.i2c0;
    // let sda = peripherals.pins.gpio2;
    // let scl = peripherals.pins.gpio3;

    // let i2c_config = I2cConfig::new().baudrate(100.kHz().into());
    // let mut i2c = I2cDriver::new(i2c, sda, scl, &i2c_config).unwrap();

    // let mut i2c_delay = Ets;

    // let mut lcd = lcd_lcm1602_i2c::Lcd::new(&mut i2c, &mut i2c_delay)
    //     .address(LCD_ADDRESS)
    //     .cursor_on(false)
    //     .rows(2)
    //     .init().unwrap();

    // lcd_clear_screen_return_cursor_home(&mut lcd);

    // if let Err(e) = Lcd::write_str(&mut lcd, &format!("Initializing...")) {
    //     eprintln!("Error writing init to screen: {:?}", e);
    // }

    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default())?);

    
    connect_to_wifi(peripherals.modem)?;
    loop {
        let mut dht11_delay = Ets;
        match dht11.perform_measurement(&mut dht11_delay) {
            Ok(measurement) => {
                temperatures.push(measurement.temperature as f32 / 2.0);
                humidities.push(measurement.humidity as f32 / 10.0);

                if temperatures.len() == 10 {
                    median_temperature = (temperatures.iter().sum::<f32>() / 10.0) as f32;
                    median_humidity = (humidities.iter().sum::<f32>() / 10.0) as f32;

                    println!(
                        "temp: {}C, humidity: {}%",
                        median_temperature, median_humidity
                    );

                    // lcd_clear_screen_return_cursor_home(&mut lcd);

                    // if let Err(e) = Lcd::write_str(&mut lcd, &format!("Temp: {:.2}C", median_temperature)) { 
                    //     eprintln!("Error writing temperature to screen: {:?}", e);
                    // }

                    // if let Err(e) = Lcd::set_cursor(&mut lcd, 1, 0) {
                    //     eprintln!("Error setting cursor to second line: {:?}", e);
                    // }

                    // if let Err(e) = Lcd::write_str(&mut lcd, &format!("Humd: {:.2}%", median_humidity)) {
                    //     eprintln!("Error writing humidity to screen: {:?}", e);
                    // }

                    // if let Err(e) = post("https://jsonplaceholder.typicode.com/posts", &mut client) {
                    //     eprintln!("Post error: {:?}", e);
                    // }
                    let data = SensorData {
                        humidity: median_humidity,
                        temperature: median_temperature,
                    };

                    println!("Before POST request >>");

                    post_request(&mut client, &serde_json::to_string(&data).unwrap())?;

                    temperatures.clear();
                    humidities.clear();
                }
            }
            Err(e) => println!("DHT Error: {:?}", e),
        }

        FreeRtos::delay_ms(2000);
    }
    Ok(())
}
fn connect_to_wifi(modem: Modem) -> anyhow::Result<()> {
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let wifi_driver = EspWifi::new(modem, sys_loop.clone(), Some(nvs)).unwrap();
    let mut wifi = BlockingWifi::wrap(wifi_driver, sys_loop)?;

    let ssid: String<32> = String::try_from("Yettel_FAD494").unwrap();
    let password: String<64> = String::try_from("Fz3ZRAbQ").unwrap();

    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid,
        password,
        // auth_method: AuthMethod::WPA2WPA3Personal,k
        bssid: None,
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;

    wifi.start()?;
    info!("WiFi started!");

    wifi.connect()?;
    info!("Wifi Connected!");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(())
}

fn lcd_clear_screen_return_cursor_home(lcd: &mut Lcd<I2cDriver, Ets>) {
    if let Err(e) = Lcd::return_home(lcd) {
        eprintln!("Error returning cursor: {:?}", e);
    }

    if let Err(e) = Lcd::clear(lcd) {
        eprintln!("Error clearing LCD: {:?}", e);
    }
}

/// Send an HTTP POST request.
fn post_request(client: &mut HttpClient<EspHttpConnection>, payload: &str) -> anyhow::Result<()> {
    println!("Inside POST request >>");
    // Prepare headers and URL
    let content_length_header = format!("{}", payload.len());
    let headers = [
        ("content-type", "text/plain"),
        ("content-length", &*content_length_header),
    ];
    let url = "http://httpbin.org/post";

    println!("Before REAL POST request >>");
    // Send request
    let mut request = client.post(url, &headers)?;
    request.write_all(payload.as_bytes())?;
    request.flush()?;
    info!("-> POST {}", url);
    let mut response = request.submit()?;

    // Process response
    let status = response.status();
    info!("<- {}", status);
    let mut buf = [0u8; 1024];
    let bytes_read = io::try_read_full(&mut response, &mut buf).map_err(|e| e.0)?;
    info!("Read {} bytes", bytes_read);
    match std::str::from_utf8(&buf[0..bytes_read]) {
        Ok(body_string) => info!(
            "Response body (truncated to {} bytes): {:?}",
            buf.len(),
            body_string
        ),
        Err(e) => error!("Error decoding response body: {}", e),
    };

    // Drain the remaining response bytes
    while response.read(&mut buf)? > 0 {}

    Ok(())
}

