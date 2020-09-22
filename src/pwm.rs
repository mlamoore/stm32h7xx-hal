//! Pulse Width Modulation (PWM)
//!
//! PWM output is avaliable for the advanced control timers (`TIM1`, `TIM8`),
//! the general purpose timers (`TIM[2-5]`) and the Low-power timers
//! (`LPTIM[1-5]`).
//!
//! TODO: `TIM[12-17]`
//!
//! Timers support up to 4 simultaneous PWM output channels
//!
//! ## Usage
//!
//! ```rust
//! let gpioa = ..; // Set up and split GPIOA
//! let pins = (
//!     gpioa.pa8.into_alternate_af1(),
//!     gpioa.pa9.into_alternate_af1(),
//!     gpioa.pa10.into_alternate_af1(),
//!     gpioa.pa11.into_alternate_af1(),
//! );
//! ```
//!
//! Then call the `pwm` function on the corresponding timer:
//!
//! ```
//!   let device: pac::Peripherals = ..;
//!
//!   // Put the timer in PWM mode using the specified pins
//!   // with a frequency of 100 hz.
//!   let (c0, c1, c2, c3) = device.TIM1.pwm(
//!       pins,
//!       100.hz(),
//!       prec,
//!       &clocks
//!   );
//!
//!   // Set the duty cycle of channel 0 to 50%
//!   c0.set_duty(c0.get_max_duty() / 2);
//!   // PWM outputs are disabled by default
//!   c0.enable()
//! ```
//!
use core::marker::PhantomData;
use core::mem::MaybeUninit;

use crate::hal;
use crate::stm32::{lptim1, lptim3};
use crate::stm32::{LPTIM1, LPTIM2, LPTIM3, LPTIM4, LPTIM5};
use crate::stm32::{
    TIM1, TIM12, TIM13, TIM14, TIM15, TIM16, TIM17, TIM2, TIM3, TIM4, TIM5,
    TIM8,
};

use crate::rcc::{rec, CoreClocks, ResetEnable};
use crate::time::{Hertz, NanoSeconds};
use crate::timer::GetClk;

use crate::gpio::gpioa::{
    PA0, PA1, PA10, PA11, PA15, PA2, PA3, PA5, PA6, PA7, PA8, PA9,
};
use crate::gpio::gpiob::{
    PB0, PB1, PB10, PB11, PB13, PB14, PB15, PB3, PB4, PB5, PB6, PB7, PB8, PB9,
};
use crate::gpio::gpioc::{PC6, PC7, PC8, PC9};
use crate::gpio::gpiod::{PD13, PD14, PD15};
use crate::gpio::gpioe::{
    PE10, PE11, PE12, PE13, PE14, PE4, PE5, PE6, PE8, PE9,
};
use crate::gpio::gpiof::{PF6, PF7, PF8, PF9};
use crate::gpio::gpiog::PG13;
use crate::gpio::gpioh::{PH10, PH11, PH12, PH13, PH14, PH15, PH6, PH9};
use crate::gpio::gpioi::{PI0, PI2, PI5, PI6, PI7};
use crate::gpio::gpioj::{PJ10, PJ11, PJ6, PJ7, PJ8, PJ9};
use crate::gpio::gpiok::{PK0, PK1};

use crate::gpio::{Alternate, AF1, AF2, AF3, AF4, AF9};

// This trait marks that a GPIO pin can be used with a specific timer channel
// TIM is the timer being used
// CHANNEL is a marker struct for the channel (or multi channels for tuples)
// Example: impl Pins<TIM1, C1> for PA8<Alternate<AF1>> { type Channel = Pwm<TIM1, C1>; }
/// Pins is a trait that marks which GPIO pins may be used as PWM channels; it should not be directly used.
/// See the device datasheet 'Pin descriptions' chapter for which pins can be used with which timer PWM channels (or look at Implementors)
pub trait Pins<TIM, CHANNEL, COMP> {
    type Channel;
}

pub trait NPins<TIM, CHANNEL> {}

/// Marker struct for PWM channel 1 on Pins trait and Pwm struct
pub struct C1;
/// Marker struct for PWM channel 2 on Pins trait and Pwm struct
pub struct C2;
/// Marker struct for PWM channel 3 on Pins trait and Pwm struct
pub struct C3;
/// Marker struct for PWM channel 4 on Pins trait and Pwm struct
pub struct C4;

/// Marker struct for pins and PWM channels that do not support complementary output
pub struct ComplementaryImpossible;
/// Marker struct for pins and PWM channels that support complementary output but are not using it
pub struct ComplementaryDisabled;
/// Marker struct for PWM channels that have complementary outputs enabled
pub struct ComplementaryEnabled;

/// Marker struct for active high IO polarity
pub struct ActiveHigh;
/// Marker struct for active low IO polarity
pub struct ActiveLow;

/// Left aligned PWM goes active at the start of the PWM period and goes inactive after the duty cycle is complete. This is the default and is supported by all timers.
pub struct AlignmentLeft;
/// Right aligned PWM goes inactive at the start of the PWM period and goes active when duty cycle time remains. It is only available on some timers.
pub struct AlignmentRight;
/// Center aligned PWM updates twice per PWM period and is active centered around one update and inactive centered around the other update. It is only available on some timers.
pub struct AlignmentCenter;

/// Pwm represents one PWM channel; it is created by calling TIM?.pwm(...) and lets you control the channel through the PwmPin trait
pub struct Pwm<TIM, CHANNEL, COMP, POL, NPOL> {
    _channel: PhantomData<CHANNEL>,
    _tim: PhantomData<TIM>,
    _complementary: PhantomData<COMP>,
    _polarity: PhantomData<POL>,
    _npolarity: PhantomData<NPOL>,
}

// automatically implement Pins trait for tuples of individual pins
macro_rules! pins_tuples {
    // Tuple of two pins
    ($(($CHA:ty, $CHB:ty)),*) => {
        $(
            impl<TIM, CHA, CHB, TA, TB> Pins<TIM, ($CHA, $CHB), (TA, TB)> for (CHA, CHB)
            where
                CHA: Pins<TIM, $CHA, TA>,
                CHB: Pins<TIM, $CHB, TB>,
            {
                type Channel = (Pwm<TIM, $CHA, TA, ActiveHigh, ActiveHigh>, Pwm<TIM, $CHB, TB, ActiveHigh, ActiveHigh>);
            }
        )*
    };
    // Tuple of three pins
    ($(($CHA:ty, $CHB:ty, $CHC:ty)),*) => {
        $(
            pins_tuples! {
                PERM3: ($CHA, $CHB, $CHC),
                PERM3: ($CHA, $CHC, $CHB),
                PERM3: ($CHB, $CHA, $CHC),
                PERM3: ($CHB, $CHC, $CHA),
                PERM3: ($CHC, $CHA, $CHB),
                PERM3: ($CHC, $CHB, $CHA)
            }
        )*
    };
    // Permutate tuple of three pins
    ($(PERM3: ($CHA:ty, $CHB:ty, $CHC:ty)),*) => {
        $(
            impl<TIM, CHA, CHB, CHC, TA, TB, TC> Pins<TIM, ($CHA, $CHB, $CHC), (TA, TB, TC)> for (CHA, CHB, CHC)
            where
                CHA: Pins<TIM, $CHA, TA>,
                CHB: Pins<TIM, $CHB, TB>,
                CHC: Pins<TIM, $CHC, TC>,
            {
                type Channel = (Pwm<TIM, $CHA, TA, ActiveHigh, ActiveHigh>, Pwm<TIM, $CHB, TB, ActiveHigh, ActiveHigh>, Pwm<TIM, $CHC, TC, ActiveHigh, ActiveHigh>);
            }
        )*
    };
    // Tuple of four pins (permutates the last 3, leaves 4th in place)
    ($(($CHD:ty, $CHA:ty, $CHB:ty, $CHC:ty)),*) => {
        $(
            pins_tuples! {
                PERM4: ($CHD, $CHA, $CHB, $CHC),
                PERM4: ($CHD, $CHA, $CHC, $CHB),
                PERM4: ($CHD, $CHB, $CHA, $CHC),
                PERM4: ($CHD, $CHB, $CHC, $CHA),
                PERM4: ($CHD, $CHC, $CHA, $CHB),
                PERM4: ($CHD, $CHC, $CHB, $CHA)
            }
        )*
    };
    // Tuple of four pins (permutates the last 3, leaves 1st in place)
    ($(PERM4: ($CHA:ty, $CHB:ty, $CHC:ty, $CHD:ty)),*) => {
        $(
            impl<TIM, CHA, CHB, CHC, CHD, TA, TB, TC, TD> Pins<TIM, ($CHA, $CHB, $CHC, $CHD), (TA, TB, TC, TD)> for (CHA, CHB, CHC, CHD)
            where
                CHA: Pins<TIM, $CHA, TA>,
                CHB: Pins<TIM, $CHB, TB>,
                CHC: Pins<TIM, $CHC, TC>,
                CHD: Pins<TIM, $CHD, TD>,
            {
                type Channel = (Pwm<TIM, $CHA, TA, ActiveHigh, ActiveHigh>, Pwm<TIM, $CHB, TB, ActiveHigh, ActiveHigh>, Pwm<TIM, $CHC, TC, ActiveHigh, ActiveHigh>, Pwm<TIM, $CHD, TD, ActiveHigh, ActiveHigh>);
            }
        )*
    }
}

pins_tuples! {
    (C1, C2),
    (C2, C1),
    (C1, C3),
    (C3, C1),
    (C1, C4),
    (C4, C1),
    (C2, C3),
    (C3, C2),
    (C2, C4),
    (C4, C2),
    (C3, C4),
    (C4, C3)
}

pins_tuples! {
    (C1, C2, C3),
    (C1, C2, C4),
    (C1, C3, C4),
    (C2, C3, C4)
}

pins_tuples! {
    (C1, C2, C3, C4),
    (C2, C1, C3, C4),
    (C3, C1, C2, C4),
    (C4, C1, C2, C3)
}

// Pin definitions, mark which pins can be used with which timers and channels
macro_rules! pins {
    // Single channel timer
    ($($TIMX:ty: OUT: [$($OUT:ty),*])+) => {
        $(
            $(
                impl Pins<$TIMX, C1, ComplementaryImpossible> for $OUT {
                    type Channel = Pwm<$TIMX, C1, ComplementaryImpossible, ActiveHigh, ActiveHigh>;
                }
            )*
        )+
    };
    // Dual channel timer
    ($($TIMX:ty: CH1($COMP1:ty): [$($CH1:ty),*] CH2($COMP2:ty): [$($CH2:ty),*]
       CH1N: [$($CH1N:ty),*] CH2N: [$($CH2N:ty),*])+) => {
        $(
            $(
                impl Pins<$TIMX, C1, $COMP1> for $CH1 {
                    type Channel = Pwm<$TIMX, C1, $COMP1, ActiveHigh, ActiveHigh>;
                }
            )*
            $(
                impl Pins<$TIMX, C2, $COMP2> for $CH2 {
                    type Channel = Pwm<$TIMX, C2, $COMP2, ActiveHigh, ActiveHigh>;
                }
            )*
            $(
                impl NPins<$TIMX, C1> for $CH1N {}
            )*
            $(
                impl NPins<$TIMX, C2> for $CH2N {}
            )*
        )+
    };
    // Quad channel timers
    ($($TIMX:ty: CH1($COMP1:ty): [$($CH1:ty),*] CH2($COMP2:ty): [$($CH2:ty),*] CH3($COMP3:ty): [$($CH3:ty),*] CH4($COMP4:ty): [$($CH4:ty),*]
       CH1N: [$($CH1N:ty),*] CH2N: [$($CH2N:ty),*] CH3N: [$($CH3N:ty),*] CH4N: [$($CH4N:ty),*])+) => {
        $(
            $(
                impl Pins<$TIMX, C1, $COMP1> for $CH1 {
                    type Channel = Pwm<$TIMX, C1, $COMP1, ActiveHigh, ActiveHigh>;
                }
            )*
            $(
                impl Pins<$TIMX, C2, $COMP2> for $CH2 {
                    type Channel = Pwm<$TIMX, C2, $COMP2, ActiveHigh, ActiveHigh>;
                }
            )*
            $(
                impl Pins<$TIMX, C3, $COMP3> for $CH3 {
                    type Channel = Pwm<$TIMX, C3, $COMP3, ActiveHigh, ActiveHigh>;
                }
            )*
            $(
                impl Pins<$TIMX, C4, $COMP4> for $CH4 {
                    type Channel = Pwm<$TIMX, C4, $COMP4, ActiveHigh, ActiveHigh>;
                }
            )*
            $(
                impl NPins<$TIMX, C1> for $CH1N {}
            )*
            $(
                impl NPins<$TIMX, C2> for $CH2N {}
            )*
            $(
                impl NPins<$TIMX, C3> for $CH3N {}
            )*
            $(
                impl NPins<$TIMX, C4> for $CH4N {}
            )*
        )+
    }
}
// Single channel timers
pins! {
    LPTIM1:
        OUT: [
            PD13<Alternate<AF1>>,
            PG13<Alternate<AF1>>
        ]
    LPTIM2:
        OUT: [
            PB13<Alternate<AF3>>
        ]
    LPTIM3:
        OUT: [
            PA1<Alternate<AF3>>
        ]
    LPTIM4:
        OUT: [
            PA2<Alternate<AF3>>
        ]
    LPTIM5:
        OUT: [
            PA3<Alternate<AF3>>
        ]
}
// Dual channel timers
pins! {
    TIM12:
        CH1(ComplementaryImpossible): [
            PB14<Alternate<AF2>>,
            PH6<Alternate<AF2>>
        ]
        CH2(ComplementaryImpossible): [
            PB15<Alternate<AF2>>,
            PH9<Alternate<AF2>>
        ]
        CH1N: []
        CH2N: []
    TIM13:
        CH1(ComplementaryImpossible): [
            PA6<Alternate<AF9>>,
            PF8<Alternate<AF9>>
        ]
        CH2(ComplementaryImpossible): []
        CH1N: []
        CH2N: []
    TIM14:
        CH1(ComplementaryImpossible): [
            PA7<Alternate<AF9>>,
            PF9<Alternate<AF9>>
        ]
        CH2(ComplementaryImpossible): []
        CH1N: []
        CH2N: []
    TIM15:
        CH1(ComplementaryDisabled): [
            PA2<Alternate<AF4>>,
            PE5<Alternate<AF4>>
        ]
        CH2(ComplementaryImpossible): [
            PA3<Alternate<AF4>>,
            PE6<Alternate<AF4>>
        ]
        CH1N: [
            PA1<Alternate<AF4>>,
            PE4<Alternate<AF4>>
        ]
        CH2N: []
}
// More single channel timers
pins! {
    TIM16:
        CH1(ComplementaryDisabled): [
            PB8<Alternate<AF1>>,
            PF6<Alternate<AF1>>
        ]
        CH2(ComplementaryImpossible): []
        CH1N: [
            PB6<Alternate<AF1>>,
            PF8<Alternate<AF1>>
        ]
        CH2N: []
    TIM17:
        CH1(ComplementaryDisabled): [
            PB9<Alternate<AF1>>,
            PF7<Alternate<AF1>>
        ]
        CH2(ComplementaryImpossible): []
        CH1N: [
            PB7<Alternate<AF1>>,
            PF9<Alternate<AF1>>
        ]
        CH2N: []
}
// Quad channel timers
pins! {
    TIM1:
        CH1(ComplementaryDisabled): [
            PA8<Alternate<AF1>>,
            PE9<Alternate<AF1>>,
            PK1<Alternate<AF1>>
        ]
        CH2(ComplementaryDisabled): [
            PA9<Alternate<AF1>>,
            PE11<Alternate<AF1>>,
            PJ11<Alternate<AF1>>
        ]
        CH3(ComplementaryDisabled): [
            PA10<Alternate<AF1>>,
            PE13<Alternate<AF1>>,
            PJ9<Alternate<AF1>>
        ]
        CH4(ComplementaryImpossible): [
            PA11<Alternate<AF1>>,
            PE14<Alternate<AF1>>
        ]
        CH1N: [
            PA7<Alternate<AF1>>,
            PB13<Alternate<AF1>>,
            PE8<Alternate<AF1>>,
            PK0<Alternate<AF1>>
        ]
        CH2N: [
            PB0<Alternate<AF1>>,
            PB14<Alternate<AF1>>,
            PE10<Alternate<AF1>>,
            PJ10<Alternate<AF1>>
        ]
        CH3N: [
            PB1<Alternate<AF1>>,
            PB15<Alternate<AF1>>,
            PE12<Alternate<AF1>>,
            PJ8<Alternate<AF1>>
        ]
        CH4N: []
    TIM2:
        CH1(ComplementaryImpossible): [
            PA0<Alternate<AF1>>,
            PA5<Alternate<AF1>>,
            PA15<Alternate<AF1>>
        ]
        CH2(ComplementaryImpossible): [
            PA1<Alternate<AF1>>,
            PB3<Alternate<AF1>>
        ]
        CH3(ComplementaryImpossible): [
            PA2<Alternate<AF1>>,
            PB10<Alternate<AF1>>
        ]
        CH4(ComplementaryImpossible): [
            PA3<Alternate<AF1>>,
            PB11<Alternate<AF1>>
        ]
        CH1N: []
        CH2N: []
        CH3N: []
        CH4N: []
    TIM3:
        CH1(ComplementaryImpossible): [
            PA6<Alternate<AF2>>,
            PB4<Alternate<AF2>>,
            PC6<Alternate<AF2>>
        ]
        CH2(ComplementaryImpossible): [
            PA7<Alternate<AF2>>,
            PB5<Alternate<AF2>>,
            PC7<Alternate<AF2>>
        ]
        CH3(ComplementaryImpossible): [
            PB0<Alternate<AF2>>,
            PC8<Alternate<AF2>>
        ]
        CH4(ComplementaryImpossible): [
            PB1<Alternate<AF2>>,
            PC9<Alternate<AF2>>
        ]
        CH1N: []
        CH2N: []
        CH3N: []
        CH4N: []
    TIM4:
        CH1(ComplementaryImpossible): [
            PB6<Alternate<AF2>>
        ]
        CH2(ComplementaryImpossible): [
            PB7<Alternate<AF2>>,
            PD13<Alternate<AF2>>
        ]
        CH3(ComplementaryImpossible): [
            PB8<Alternate<AF2>>,
            PD14<Alternate<AF2>>
        ]
        CH4(ComplementaryImpossible): [
            PB9<Alternate<AF2>>,
            PD15<Alternate<AF2>>
        ]
        CH1N: []
        CH2N: []
        CH3N: []
        CH4N: []
    TIM5:
        CH1(ComplementaryImpossible): [
            PA0<Alternate<AF2>>,
            PH10<Alternate<AF2>>
        ]
        CH2(ComplementaryImpossible): [
            PA1<Alternate<AF2>>,
            PH11<Alternate<AF2>>
        ]
        CH3(ComplementaryImpossible): [
            PA2<Alternate<AF2>>,
            PH12<Alternate<AF2>>
        ]
        CH4(ComplementaryImpossible): [
            PA3<Alternate<AF2>>,
            PI0<Alternate<AF2>>
        ]
        CH1N: []
        CH2N: []
        CH3N: []
        CH4N: []
    TIM8:
        CH1(ComplementaryDisabled): [
            PC6<Alternate<AF3>>,
            PI5<Alternate<AF3>>,
            PJ8<Alternate<AF3>>
        ]
        CH2(ComplementaryDisabled): [
            PC7<Alternate<AF3>>,
            PI6<Alternate<AF3>>,
            PJ6<Alternate<AF3>>,
            PJ10<Alternate<AF3>>
        ]
        CH3(ComplementaryDisabled): [
            PC8<Alternate<AF3>>,
            PI7<Alternate<AF3>>,
            PK0<Alternate<AF3>>
        ]
        CH4(ComplementaryImpossible): [
            PC9<Alternate<AF3>>,
            PI2<Alternate<AF3>>
        ]
        CH1N: [
            PA5<Alternate<AF3>>,
            PA7<Alternate<AF3>>,
            PH13<Alternate<AF3>>,
            PJ9<Alternate<AF3>>
        ]
        CH2N: [
            PB0<Alternate<AF3>>,
            PB14<Alternate<AF3>>,
            PH14<Alternate<AF3>>,
            PJ7<Alternate<AF3>>,
            PJ11<Alternate<AF3>>
        ]
        CH3N: [
            PB1<Alternate<AF3>>,
            PB15<Alternate<AF3>>,
            PH15<Alternate<AF3>>,
            PK1<Alternate<AF3>>
        ]
        CH4N: []
}

pub struct PwmBuilder<TIM, PINS, CHANNEL, COMP, ALIGNMENT, WIDTH> {
    _tim: PhantomData<TIM>,
    _pins: PhantomData<PINS>,
    _channel: PhantomData<CHANNEL>,
    _comp: PhantomData<COMP>,
    _alignment: PhantomData<ALIGNMENT>,
    _width: PhantomData<WIDTH>,
    base_freq: Hertz,
}

impl<PINS, CHANNEL, COMP, ALIGNMENT>
    PwmBuilder<TIM1, PINS, CHANNEL, COMP, ALIGNMENT, u16>
where
    PINS: Pins<TIM1, CHANNEL, COMP>,
{
    pub fn finalize(self) -> PINS::Channel {
        let tim = unsafe { &*TIM1::ptr() };
        // BDTR: Advanced-control timers
        // Set CCxP = OCxREF / CCxNP = !OCxREF
        // Refer to RM0433 Rev 6 - Table 324.
        tim.bdtr.write(|w| w.moe().enabled());

        tim.cr1.write(|w| w.cen().enabled());

        unsafe { MaybeUninit::<PINS::Channel>::uninit().assume_init() }
    }

    pub fn with_deadtime(self, deadtime: NanoSeconds) -> Self {
        let tim = unsafe { &*TIM1::ptr() };

        let freq = self.base_freq.0;

        // tDTS is based on tCK_INT which is before the prescaler

        // ticks = ns * GHz = ns * Hz / 1e9
        // deadtime.0 could be tens to thousands of ns
        // freq could be up to 240,000,000 Hz
        // Need to avoid overflow but don't want to divide down frequency in case of non-integer-MHz frequency
        // Cortex-M7 has 32x32->64 multiply but no 64-bit divide
        // Divide by 100000 then 10000 by multiplying and shifting
        assert!(deadtime.0 <= 1_789_556);
        let deadtime_ticks = deadtime.0 as u64 * freq as u64;
        let deadtime_ticks = deadtime_ticks * 42950;
        let deadtime_ticks = (deadtime_ticks >> 32) as u32;
        let deadtime_ticks = deadtime_ticks as u64 * 429497;
        let deadtime_ticks = (deadtime_ticks >> 32) as u32;

        assert!(deadtime_ticks <= 4032);

        let deadtime_ticks = deadtime_ticks as u32;

        // TODO finish
        // Choose CR1 CKD divider of 1, 2, or 4 to determine tDTS
        // Choose BDTR DTG bits to match deadtime_ticks

        self
    }
}

impl<PINS, CHANNEL, COMP>
    PwmBuilder<TIM1, PINS, CHANNEL, COMP, AlignmentLeft, u16>
where
    PINS: Pins<TIM1, CHANNEL, COMP>,
{
    pub fn center_aligned(
        self,
    ) -> PwmBuilder<TIM1, PINS, CHANNEL, COMP, AlignmentCenter, u16> {
        let tim = unsafe { &*TIM1::ptr() };

        // Center aligned needs half the period for the same frequency
        let period = tim.arr.read().arr().bits();

        tim.arr.write(|w| w.arr().bits(period / 2));

        tim.cr1.write(|w| w.cms().center_aligned3());

        PwmBuilder {
            _tim: PhantomData,
            _pins: PhantomData,
            _channel: PhantomData,
            _comp: PhantomData,
            _alignment: PhantomData,
            _width: PhantomData,
            base_freq: self.base_freq,
        }
    }

    pub fn right_aligned(
        self,
    ) -> PwmBuilder<TIM1, PINS, CHANNEL, COMP, AlignmentRight, u16> {
        let tim = unsafe { &*TIM1::ptr() };

        // Right aligned is the same as left, just counting down instead of up
        tim.cr1.write(|w| w.dir().down());

        PwmBuilder {
            _tim: PhantomData,
            _pins: PhantomData,
            _channel: PhantomData,
            _comp: PhantomData,
            _alignment: PhantomData,
            _width: PhantomData,
            base_freq: self.base_freq,
        }
    }

    pub fn left_aligned(
        self,
    ) -> PwmBuilder<TIM1, PINS, CHANNEL, COMP, AlignmentLeft, u16> {
        self
    }
}

// PwmExt trait
/// Allows the pwm() method to be added to the peripheral register structs from the device crate
pub trait PwmExt: Sized {
    type Rec: ResetEnable;

    fn pwm<PINS, T, U, V>(
        self,
        _pins: PINS,
        frequency: T,
        prec: Self::Rec,
        clocks: &CoreClocks,
    ) -> PINS::Channel
    where
        PINS: Pins<Self, U, V>,
        T: Into<Hertz>;
}

pub trait PwmAdvExt: Sized {
    type Rec: ResetEnable;

    fn advanced_pwm<PINS, CHANNEL, COMP, T>(
        self,
        _pins: PINS,
        frequency: T,
        prec: Self::Rec,
        clocks: &CoreClocks,
    ) -> PwmBuilder<Self, PINS, CHANNEL, COMP, AlignmentLeft, u16>
    where
        PINS: Pins<Self, CHANNEL, COMP>,
        T: Into<Hertz>;
}

impl PwmAdvExt for TIM1 {
    type Rec = rec::Tim1;

    fn advanced_pwm<PINS, CHANNEL, COMP, T>(
        self,
        _pins: PINS,
        freq: T,
        prec: Self::Rec,
        clocks: &CoreClocks,
    ) -> PwmBuilder<Self, PINS, CHANNEL, COMP, AlignmentLeft, u16>
    where
        PINS: Pins<Self, CHANNEL, COMP>,
        T: Into<Hertz>,
    {
        prec.enable().reset();

        let clk = TIM1::get_clk(clocks)
            .expect("Timer input clock not running!")
            .0;
        let freq: Hertz = freq.into();
        let reload: u32 = clk / freq.0; // u32

        let prescale = {
            // Division factor is (PSC + 1)
            let prescale = (reload - 1) / (1 << 16);
            assert!(prescale <= 0xFFFF);
            prescale
        };

        // Write prescale
        self.psc.write(|w| w.psc().bits(prescale as u16));

        // Set TOP value
        let top = reload / (prescale + 1);
        self.arr.write(|w| w.arr().bits(top as u16));

        PwmBuilder {
            _tim: PhantomData,
            _pins: PhantomData,
            _channel: PhantomData,
            _comp: PhantomData,
            _alignment: PhantomData,
            _width: PhantomData,
            base_freq: freq,
        }
    }
}

// Implement PwmExt trait for timer
macro_rules! pwm_ext_hal {
    ($TIMX:ident: $timX:ident, $Rec:ident) => {
        impl PwmExt for $TIMX {
            type Rec = rec::$Rec;

            fn pwm<PINS, T, U, V>(
                self,
                pins: PINS,
                frequency: T,
                prec: rec::$Rec,
                clocks: &CoreClocks,
            ) -> PINS::Channel
            where
                PINS: Pins<Self, U, V>,
                T: Into<Hertz>,
            {
                $timX(self, pins, frequency.into(), prec, clocks)
            }
        }
    };
}

// Implement PWM configuration for timer
macro_rules! tim_hal {
    ($($TIMX:ident: ($timX:ident, $Rec:ident,
                     $typ:ty, $bits:expr $(,$bdtr:ident)*),)+) => {
        $(
            pwm_ext_hal!($TIMX: $timX, $Rec);

            /// Configures PWM
            fn $timX<PINS, T, U>(
                tim: $TIMX,
                _pins: PINS,
                freq: Hertz,
                prec: rec::$Rec,
                clocks: &CoreClocks,
            ) -> PINS::Channel
            where
                PINS: Pins<$TIMX, T, U>,
            {
                prec.enable().reset();

                let clk = $TIMX::get_clk(clocks)
                    .expect("Timer input clock not running!").0;
                let freq = freq.0;
                let reload : u32 = clk / freq; // u32

                let prescale = match $bits {
                    16 => {
                        // Division factor is (PSC + 1)
                        let prescale = (reload - 1) / (1 << 16);
                        assert!(prescale <= 0xFFFF);
                        prescale
                    },
                    _ => 0      // No prescale required for 32-bit timer
                };

                // Write prescale
                tim.psc.write(|w| { w.psc().bits(prescale as u16) });

                // Set TOP value
                let top = reload / (prescale + 1);
                tim.arr.write(|w| { w.arr().bits(top as $typ) });

                // BDTR: Advanced-control timers
                $(
                    // Set CCxP = OCxREF / CCxNP = !OCxREF
                    // Refer to RM0433 Rev 6 - Table 324.
                    tim.$bdtr.write(|w|
                                   w.moe().enabled()
                    );
                )*

                tim.cr1.write(|w|
                          w.cen().enabled()
                );

                unsafe { MaybeUninit::<PINS::Channel>::uninit().assume_init() }
            }
        )+
    }
}
tim_hal! {
    TIM1: (tim1, Tim1, u16, 16, bdtr),
    TIM2: (tim2, Tim2, u32, 32),
    TIM3: (tim3, Tim3, u16, 16),
    TIM4: (tim4, Tim4, u16, 16),
    TIM5: (tim5, Tim5, u32, 32),
    TIM8: (tim8, Tim8, u16, 16, bdtr),
}
tim_hal! {
    TIM12: (tim12, Tim12, u16, 16),
    TIM13: (tim13, Tim13, u16, 16),
    TIM14: (tim14, Tim14, u16, 16),
}
tim_hal! {
    TIM15: (tim15, Tim15, u16, 16),
    TIM16: (tim16, Tim16, u16, 16),
    TIM17: (tim17, Tim17, u16, 16),
}

// Implement PwmPin for timer channels
macro_rules! tim_pin_hal {
    // Standard pins (no complementary functionality)
    ($($TIMX:ident:
       ($CH:ty, $ccxe:ident, $ccmrx_output:ident, $ocxpe:ident, $ocxm:ident,
        $ccrx:ident, $typ:ident),)+
    ) => {
        $(
            impl hal::PwmPin for Pwm<$TIMX, $CH, ComplementaryImpossible, ActiveHigh, ActiveHigh> {
                type Duty = $typ;

                // You may not access self in the following methods!
                // See unsafe above

                fn disable(&mut self) {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.ccer.modify(|_, w| w.$ccxe().clear_bit());
                }

                fn enable(&mut self) {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.$ccmrx_output().modify(|_, w|
                        w.$ocxpe()
                            .enabled() // Enable preload
                            .$ocxm()
                            .pwm_mode1() // PWM Mode
                    );
                    tim.ccer.modify(|_, w| w.$ccxe().set_bit());
                }

                fn get_duty(&self) -> Self::Duty {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.$ccrx.read().ccr().bits()
                }

                fn get_max_duty(&self) -> Self::Duty {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.arr.read().arr().bits()
                }

                fn set_duty(&mut self, duty: Self::Duty) {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.$ccrx.write(|w| w.ccr().bits(duty));
                }
            }
        )+
    };
}
tim_pin_hal! {
    TIM1: (C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1, u16),
    TIM1: (C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2, u16),
    TIM1: (C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3, u16),
    TIM1: (C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4, u16),
}
tim_pin_hal! {
    TIM2: (C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1, u32),
    TIM2: (C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2, u32),
    TIM2: (C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3, u32),
    TIM2: (C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4, u32),
}
tim_pin_hal! {
    TIM3: (C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1, u16),
    TIM3: (C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2, u16),
    TIM3: (C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3, u16),
    TIM3: (C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4, u16),
}
tim_pin_hal! {
    TIM4: (C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1, u16),
    TIM4: (C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2, u16),
    TIM4: (C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3, u16),
    TIM4: (C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4, u16),
}
tim_pin_hal! {
    TIM5: (C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1, u32),
    TIM5: (C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2, u32),
    TIM5: (C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3, u32),
    TIM5: (C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4, u32),
}
tim_pin_hal! {
    TIM8: (C1, cc1e, ccmr1_output, oc1pe, oc1m, ccr1, u16),
    TIM8: (C2, cc2e, ccmr1_output, oc2pe, oc2m, ccr2, u16),
    TIM8: (C3, cc3e, ccmr2_output, oc3pe, oc3m, ccr3, u16),
    TIM8: (C4, cc4e, ccmr2_output, oc4pe, oc4m, ccr4, u16),
}

// Low-power timers
macro_rules! lptim_hal {
    ($($TIMX:ident: ($timX:ident, $Rec:ident, $timXpac:ident),)+) => {
        $(
            pwm_ext_hal!($TIMX: $timX, $Rec);

            /// Configures PWM signal on the LPTIM OUT pin.
            fn $timX<PINS, T, U>(
                tim: $TIMX,
                _pins: PINS,
                freq: Hertz,
                prec: rec::$Rec,
                clocks: &CoreClocks,
            ) -> PINS::Channel
            where
                PINS: Pins<$TIMX, T, U>,
            {
                prec.enable().reset();

                let clk = $TIMX::get_clk(clocks).unwrap().0;
                let freq = freq.0;
                let reload = clk / freq;
                assert!(reload < 128 * (1 << 16));

                // Calculate prescaler
                let (prescale, prescale_div) = match reload / (1 << 16) {
                    0 => ($timXpac::cfgr::PRESC_A::DIV1, 1),
                    1 => ($timXpac::cfgr::PRESC_A::DIV2, 2),
                    2..=3 => ($timXpac::cfgr::PRESC_A::DIV4, 4),
                    4..=7 => ($timXpac::cfgr::PRESC_A::DIV8, 8),
                    8..=15 => ($timXpac::cfgr::PRESC_A::DIV16, 16),
                    16..=31 => ($timXpac::cfgr::PRESC_A::DIV32, 32),
                    32..=63 => ($timXpac::cfgr::PRESC_A::DIV64, 64),
                    _ => ($timXpac::cfgr::PRESC_A::DIV128, 128),
                };

                // Calcuate reload
                let arr = reload / prescale_div;
                assert!(arr <= 0xFFFF);
                assert!(arr > 0);

                // CFGR
                tim.cfgr.modify(|_, w| w.presc().variant(prescale));

                // Enable
                tim.cr.modify(|_, w| w.enable().enabled());

                // Write ARR: LPTIM must be enabled
                tim.arr.write(|w| w.arr().bits(arr as u16));
                while !tim.isr.read().arrok().is_set() {}
                tim.icr.write(|w| w.arrokcf().clear());

                // PWM output is disabled by default, disable the
                // entire timer
                tim.cr.modify(|_, w| w.enable().disabled());

                unsafe { MaybeUninit::<PINS::Channel>::uninit().assume_init() }
            }

            impl hal::PwmPin for Pwm<$TIMX, C1, ComplementaryImpossible, ActiveHigh, ActiveHigh> {
                type Duty = u16;

                // You may not access self in the following methods!
                // See unsafe above

                fn disable(&mut self) {
                    let tim = unsafe { &*$TIMX::ptr() };

                    // LPTIM only has one output, so we disable the
                    // entire timer
                    tim.cr.modify(|_, w| w.enable().disabled());
                }

                fn enable(&mut self) {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.cr.modify(|_, w| w.cntstrt().start().enable().enabled());
                }

                fn get_duty(&self) -> u16 {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.cmp.read().cmp().bits()
                }

                fn get_max_duty(&self) -> u16 {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.arr.read().arr().bits()
                }

                fn set_duty(&mut self, duty: u16) {
                    let tim = unsafe { &*$TIMX::ptr() };

                    tim.cmp.write(|w| w.cmp().bits(duty));
                    while !tim.isr.read().cmpok().is_set() {}
                    tim.icr.write(|w| w.cmpokcf().clear());
                }
            }
        )+
    }
}

lptim_hal! {
    LPTIM1: (lptim1, Lptim1, lptim1),
    LPTIM2: (lptim2, Lptim2, lptim1),
    LPTIM3: (lptim3, Lptim3, lptim3),
    LPTIM4: (lptim4, Lptim4, lptim3),
    LPTIM5: (lptim5, Lptim5, lptim3),
}
