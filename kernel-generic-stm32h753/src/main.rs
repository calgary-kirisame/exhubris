#![no_std]
#![no_main]

use cortex_m_rt::entry;
use stm32_metapac as pac;

fn clock_setup() -> u32 {
    // The H7 starts up running from HSI at 64 MHz.
    //
    // At VOS3 (default voltage scaling) we need 1 wait state for 64 MHz.
    pac::FLASH.acr().modify(|w| {
        w.set_latency(1);
    });
    while pac::FLASH.acr().read().latency() != 1 {
        // spin
    }

    64_000
}

// Configure PLL2 for 49.152 MHz audio clock output on PLL2_P,
// routed to SAI1/DFSDM1.
//
// From HSI = 64 MHz:
//   DIVM2 = 8                  -> VCO input = 8 MHz
//   DIVN2 = 24 + 4719/8192 = 24.576  -> VCO = 196.608 MHz
//   DIVP2 = 4                   -> P output = 49.152 MHz
//
// Error: ~20 ppm.
#[cfg(feature = "audio-pll2-49mhz")]
fn configure_audio_pll2() {
    use pac::rcc::vals::{Plldiv, Pllm, Plln, Pllrge, Pllvcosel, Saisel};

    let rcc = pac::RCC;
    const PLL2: usize = 1;

    rcc.cr().modify(|w| w.set_pllon(PLL2, false));
    while rcc.cr().read().pllrdy(PLL2) {
        // spin
    }

    rcc.pllckselr().modify(|w| {
        w.set_divm(PLL2, Pllm::from_bits(8));
    });

    rcc.plldivr(PLL2).modify(|w| {
        w.set_plln(Plln::from_bits(24 - 1));
        w.set_pllp(Plldiv::from_bits(4 - 1));
    });

    // Don't enable FRACEN yet! The fractional value is latched on
    // FRACEN's 0->1 transition.
    rcc.pllcfgr().modify(|w| {
        w.set_pllfracen(PLL2, false);
        w.set_pllvcosel(PLL2, Pllvcosel::WIDEVCO);
        w.set_pllrge(PLL2, Pllrge::RANGE8);
        w.set_divpen(PLL2, true);
    });

    rcc.pllfracr(PLL2).modify(|w| {
        w.set_fracn(4719);
    });

    cortex_m::asm::dsb();

    rcc.pllcfgr().modify(|w| {
        w.set_pllfracen(PLL2, true);
    });

    rcc.cr().modify(|w| w.set_pllon(PLL2, true));
    while !rcc.cr().read().pllrdy(PLL2) {
        // spin
    }

    rcc.d2ccip1r().modify(|w| {
        w.set_sai1sel(Saisel::PLL2_P);
    });
}

#[entry]
fn main() -> ! {
    let cycles_per_ms = clock_setup();

    #[cfg(feature = "audio-pll2-49mhz")]
    configure_audio_pll2();

    if cfg!(feature = "clock-hsi48-on") {
        pac::RCC.cr().modify(|w| w.set_hsi48on(true));
        while !pac::RCC.cr().read().hsi48rdy() {
            // spin
        }
        pac::RCC
            .d2ccip2r()
            .modify(|w| w.set_usbsel(pac::rcc::vals::Usbsel::HSI48));
    }

    if cfg!(feature = "pwr-vddusb-valid") {
        // On LQFP144 there's no VDD33USB pin, so USB33RDY will never
        // assert. Don't enable USBREGEN (no output pin) and don't wait
        // for USB33RDY.
        pac::PWR.cr3().modify(|w| {
            w.set_usbregen(false);
            w.set_usb33den(true);
        });
    }

    unsafe { hubris_kern::startup::start_kernel(cycles_per_ms) }
}
