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
    let _gpioa = dp.GPIOA.split(ccdr.peripheral.GPIOA);
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);

    // Select PWM output pins
    let tim3_pins = gpiob.pb0.into_alternate_af2();
    let tim1_pins = (gpioe.pe13.into_alternate_af1(), gpioe.pe11.into_alternate_af1(), gpioe.pe9.into_alternate_af1());
    let _tim1_npins = (gpioe.pe12.into_alternate_af1(), gpioe.pe10.into_alternate_af1(), gpioe.pe8.into_alternate_af1());

    info!("");
    info!("stm32h7xx-hal example - PWM");
    info!("");

    // Configure PWM at 10kHz
    let mut tim3_pwm = dp.TIM3.pwm(tim3_pins, 1.hz(), ccdr.peripheral.TIM3, &ccdr.clocks);

    // Output PWM on PA8
    let max = tim3_pwm.get_max_duty();
    tim3_pwm.set_duty(max / 4);

    info!("25%");
    tim3_pwm.enable();

    let (mut t1c1, mut t1c2, mut t1c3) = dp.TIM1.pwm(tim1_pins, 2.hz(), ccdr.peripheral.TIM1, &ccdr.clocks);

    let max = t1c1.get_max_duty();
    t1c1.set_duty( max / 4 * 1);
    t1c2.set_duty( max / 4 * 2);
    t1c3.set_duty( max / 4 * 3);
    
    t1c1.enable();
    t1c2.enable();
    t1c3.enable();

    loop {}
}
