#![no_main]
#![no_std]

extern crate alloc;

use cortex_m::prelude::_embedded_hal_blocking_delay_DelayMs as DelayMs;
use defmt::Format;
use nrf9160_hal::{
    pac::{interrupt, CorePeripherals, Interrupt, NVIC},
    Delay,
};
use nrfxlib::modem;
use serde::{Deserialize, Serialize};

use defmt_rtt as _; // global logger
use panic_probe as _;
use tinyrlibc as _;

mod config;
mod golioth;
mod heap;
mod keys;
mod utils;

#[interrupt]
fn EGU1() {
    nrfxlib::application_irq_handler();
    cortex_m::asm::sev();
}

#[interrupt]
fn EGU2() {
    nrfxlib::trace_irq_handler();
    cortex_m::asm::sev();
}

#[interrupt]
fn IPC() {
    nrfxlib::ipc_irq_handler();
    cortex_m::asm::sev();
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut core = CorePeripherals::take().unwrap();

    // Initialize the heap.
    heap::init();

    unsafe {
        NVIC::unmask(Interrupt::EGU1);
        NVIC::unmask(Interrupt::EGU2);
        NVIC::unmask(Interrupt::IPC);

        core.NVIC.set_priority(Interrupt::EGU1, 4 << 5);
        core.NVIC.set_priority(Interrupt::EGU2, 4 << 5);
        core.NVIC.set_priority(Interrupt::IPC, 0 << 5);
    }

    // Workaround for https://infocenter.nordicsemi.com/index.jsp?topic=%2Ferrata_nRF9160_EngA%2FERR%2FnRF9160%2FEngineeringA%2Flatest%2Fanomaly_160_17.html
    unsafe {
        core::ptr::write_volatile(0x4000_5C04 as *mut u32, 0x02);
    }

    let mut delay = Delay::new(core.SYST);

    defmt::info!("initializing nrfxlib");

    nrfxlib::init().unwrap();
    modem::flight_mode().unwrap();

    keys::install_psk_and_psk_id(config::SECURITY_TAG, config::PSK_ID, config::PSK);

    modem::on().unwrap();

    defmt::info!("connecting to lte");

    modem::wait_for_lte().unwrap();

    defmt::info!("connecting to Golioth");

    run(&mut delay).unwrap();

    utils::exit()
}

fn run(delay: &mut impl DelayMs<u32>) -> Result<(), golioth::Error> {
    let mut golioth = golioth::Golioth::new()?;

    #[derive(Format, Deserialize)]
    struct Leds {
        #[serde(rename(deserialize = "0"))]
        led0: bool,
    }

    let leds: u32 = golioth.lightdb_get("test")?;

    defmt::info!("leds: {:?}", leds);

    #[derive(Serialize)]
    struct Counter {
        i: usize,
    }

    for i in 0.. {
        defmt::info!("writing to /counter");
        golioth.lightdb_set("counter", Counter { i })?;

        delay.delay_ms(5_000);
    }

    Ok(())
}
