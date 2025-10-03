# co2 monitor

Measure temperature/humidity/co2 using esp32 and scd41 sensor, then display the result with e-ink screen

## run

> NOTE: require [toolchain](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html) and CH340 driver

```bash
cargo run --package co2-monitor --bin co2-monitor
```

## hardware

- esp32: https://sensirion.com/media/documents/48C4B7FB/67FE0194/CD_DS_SCD4x_Datasheet_D1.pdf
- e-paper: 
  - https://www.waveshare.net/wiki/Pico-ePaper-2.9
  - https://www.waveshare.net/w/upload/7/79/2.9inch-e-paper-v2-specification.pdf

## demo

### monitor with scd41

<img src="assets/co2-result.jpg" width="512">

### show img with e-ink

> ref: `src/bin/main.rs#_backup_for_img_display`

origin: 

https://palette.clearrave.co.jp/product/kokoiro/

<img src="assets/kujo-origin.jpg" width="512">

origin(gray):

<img src="assets/kujo.bmp" width="512">

result:

<img src="assets/kujo-result.jpg" width="512">

## TODOs

- metric over network(for prometheus/vmetric)
