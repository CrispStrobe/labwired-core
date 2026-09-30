/*
 * nRF52840 fidelity cases. One source, three images.
 *
 *   nrf-control (default) : legacy UART0 TXD prints BENCH_NRF_OK. Positive
 *                           control for this chip's console path.
 *
 *   rtcclock (-DRTC_CPU)  : starts RTC0, then spins far fewer CPU cycles than
 *                           one 32.768 kHz tick (64 MHz / 32768 = 1953). Prints
 *                           BENCH_RTC_CPU only if COUNTER moved. Silicon's RTC
 *                           is on LFCLK, not the CPU clock, so it must not.
 *
 *   flashgate (-DFLASH_NOWEN)
 *                         : stores one byte in flash without NVMC CONFIG.WEN.
 *                           Prints BENCH_FLASH_OK only if the byte changed.
 *                           Silicon ignores a program while write mode is off.
 */
#include <stdint.h>

#define REG32(addr) (*(volatile uint32_t *) (addr))

#define UART0_BASE 0x40002000u
#define UART_STARTTX REG32(UART0_BASE + 0x008u)
#define UART_TXDRDY REG32(UART0_BASE + 0x11Cu)
#define UART_ENABLE REG32(UART0_BASE + 0x500u)
#define UART_PSEL_TXD REG32(UART0_BASE + 0x50Cu)
#define UART_TXD REG32(UART0_BASE + 0x51Cu)
#define UART_BAUDRATE REG32(UART0_BASE + 0x524u)
#define UART_ENABLE_LEGACY 4u
#define UART_BAUD_115200 0x01D7E000u

#define RTC0_BASE 0x4000B000u
#define RTC_TASKS_START REG32(RTC0_BASE + 0x000u)
#define RTC_COUNTER REG32(RTC0_BASE + 0x504u)

/* Erased flash, well past this image, inside the 1 MB map. */
#define FLASH_PROBE ((volatile uint8_t *) 0x000E0000u)

static void uart_init(void)
{
    UART_PSEL_TXD = 6u; /* P0.06 */
    UART_BAUDRATE = UART_BAUD_115200;
    UART_ENABLE = UART_ENABLE_LEGACY;
    UART_STARTTX = 1u;
}

static void uart_putc(char c)
{
    UART_TXDRDY = 0u;
    UART_TXD = (uint32_t) (uint8_t) c;
    for (uint32_t i = 0; i < 1000u && UART_TXDRDY == 0u; i++) {
    }
}

static void uart_puts(const char *s)
{
    while (*s) uart_putc(*s++);
}

int main(void)
{
    uart_init();
    uart_puts("BENCH_BANNER\n");

#if defined(RTC_CPU)
    RTC_TASKS_START = 1u;
    uint32_t before = RTC_COUNTER;
    __asm volatile(
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n" ::: "memory");
    if (RTC_COUNTER != before) {
        uart_puts("BENCH_RTC_CPU\n");
    }
#elif defined(FLASH_NOWEN)
    volatile uint8_t *cell = FLASH_PROBE;
    uint8_t before = *cell;
    *cell = 0xA5u;
    if (*cell != before) {
        uart_puts("BENCH_FLASH_OK\n");
    }
#else
    uart_puts("BENCH_NRF_OK\n");
#endif

    for (;;) {
    }
}
