/* IO-Link MASTER firmware-under-test for the simulated STM32L476.
 *
 * Brings up the real iolinki-master stack over the USART2 PHY and runs its
 * cyclic loop against a real iolinki DEVICE running as firmware on a separate
 * simulated chip, wired C/Q-to-C/Q by a UartCrossLink. Reaches OPERATE and
 * reads the device's process data.
 *
 * Built as a standard STM32CubeL4 project (CMSIS startup/system/linker), with
 * peripherals driven through the CMSIS register definitions — no hand-computed
 * register addresses.
 *
 * Observability for the host-side test harness:
 *   g_master_state — current iolink_master_state_t (3 == OPERATE)
 *   g_master_pd    — latest PD-in byte from the device
 * The integration test resolves both symbols from the ELF and reads them via
 * the bus; the firmware never has to format a UART message to be observed.
 */
#include "stm32l476xx.h"
#include "iolinki_master/master.h"
#include "phy_labwired.h"
#include "debug_uart.h"
#include <stdint.h>

/* The CMSIS startup calls __libc_init_array for C++/constructor init-array
 * entries; this firmware has none, and -nostartfiles drops the crt object that
 * defines _init, so provide an empty implementation. */
void __libc_init_array(void) {}

volatile uint8_t g_master_state = 0xFFu; /* 0xFF = not yet initialized */
volatile uint8_t g_master_pd = 0xFFu;

/* RCC (STM32L4, RM0351 §6.4) — the simulator models clock-gating, so USART1
 * (debug, APB2) and USART2 (IO-Link PHY, APB1) are unclocked out of reset and
 * their registers read/write as no-ops until the matching enable bit is set. */
static void rcc_init(void) {
    RCC->APB2ENR |= RCC_APB2ENR_USART1EN;   /* debug UART */
    RCC->APB1ENR1 |= RCC_APB1ENR1_USART2EN; /* IO-Link C/Q PHY */
    /* GPIOA carries both of them: PA2/PA3 are the USART2 C/Q pair and PA9 is
     * the debug console TX. MODER and AFR are dead until the port is clocked
     * (RM0351 6.4.17), so without this the pads never leave the GPIO block and
     * a probe on the C/Q line shows the idle latch, not the serial waveform. */
    RCC->AHB2ENR |= RCC_AHB2ENR_GPIOAEN;
}

/* The master stack schedules cycles against `now_100us`. A loop-counter clock
 * that just adds 20 per iteration is not a clock: this loop is far faster than
 * the wire, so the stack's response deadlines and cycle pacing bore no relation
 * to modeled time, it restarted exchanges mid-reply, and the device dropped the
 * link. DWT->CYCCNT is a true cycle counter in the simulator (derived from the
 * bus cycle clock, 4 MHz here), so now_100us tracks the modeled wire exactly. */
#define CYCLES_PER_100US 400u

static void time_init(void) {
    CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
    DWT->CYCCNT = 0u;
    DWT->CTRL |= DWT_CTRL_CYCCNTENA_Msk;
}

int main(void) {
    rcc_init();
    dbg_uart_init();
    time_init();

    iolink_master_controller_t ctrl;
    iolink_master_port_t port;
    iolink_phy_api_t phy = *phy_labwired_master_phy();
    iolink_master_config_t cfg = phy_labwired_master_config();

    if (iolink_master_controller_init(&ctrl, &port, 1u, &phy, &cfg) != 0) {
        g_master_state = 0xEEu; /* init failure sentinel */
        for (;;) {
        }
    }

    uint32_t next_tick = 0u;
    for (;;) {
        /* One scheduler tick per 2 ms of modeled time. Ticking every loop
         * iteration is not harmless: startup steps are not cycle-paced, so the
         * stack re-sends a pending DeviceOperate write on every tick until its
         * CKS-only reply is consumed, and the duplicate Type-0 frame is a type
         * error to the device (link reset). Gating the tick on DWT->CYCCNT
         * gives the device its round-trip time. */
        uint32_t now = DWT->CYCCNT / CYCLES_PER_100US;
        if ((int32_t)(now - next_tick) >= 0) {
            iolink_master_controller_tick_at(&ctrl, now);
            next_tick = now + 20u;
        }

        iolink_master_port_t *p = 0;
        if (iolink_master_controller_get_port(&ctrl, 0u, &p) == 0 && p) {
            g_master_state = (uint8_t)iolink_master_get_state(p);

            uint8_t pd[1] = {0u};
            uint8_t n = 0u;
            if (iolink_master_get_pd_in(p, pd, sizeof(pd), &n) == 0 && n >= 1u) {
                g_master_pd = pd[0];
            }
        }
    }
}
