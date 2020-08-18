#![deny(warnings)]
#![deny(unsafe_code)]
#![no_main]
#![no_std]

//use cortex_m::asm;
use cortex_m_rt::entry;
#[path = "utilities/logger.rs"]
mod logger;
use stm32h7xx_hal::{pac, prelude::*};

use log::info;

#[entry]
fn main() -> ! {
    logger::init();
    let dp = pac::Peripherals::take().expect("Cannot take peripherals");

    // Constrain and Freeze power
    info!("Setup PWR...                  ");
    let pwr = dp.PWR.constrain();
    let vos = pwr.freeze();

    // Constrain and Freeze clock
    info!("Setup RCC...                  ");
    let rcc = dp.RCC.constrain();
    let ccdr = rcc.sys_ck(8.mhz()).freeze(vos, &dp.SYSCFG);

    // Acquire the GPIOA and B peripheral. This also enables the clock for
    // GPIOA and B in the RCC register.
    let gpioa = dp.GPIOA.split(ccdr.peripheral.GPIOA);
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);

    // Select PWM output pins
    let tim3_pins = gpiob.pb0.into_alternate_af2();
    let _tim1_pins = gpioa.pa8.into_alternate_af1();
    let _tim1_npins = gpiob.pb14.into_alternate_af1();

    info!("");
    info!("stm32h7xx-hal example - PWM");
    info!("");

    // Configure PWM at 10kHz
    let mut tim3_pwm =
        dp.TIM3
            .pwm(tim3_pins, 1.hz(), ccdr.peripheral.TIM3, &ccdr.clocks);

    // Output PWM on PA8
    let max = tim3_pwm.get_max_duty();
    tim3_pwm.set_duty(max / 4);

    info!("25%");
    tim3_pwm.enable();

    loop {}
}
