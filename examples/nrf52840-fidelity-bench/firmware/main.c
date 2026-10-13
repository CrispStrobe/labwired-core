/*
 * nRF52840 images. One source, four ELFs. Each image prints its marker.
 *
 *   nrf-control (default)   BENCH_NRF_OK      legacy UART0 TXD
 *   uarttime (-DUART_TIME)  BENCH_UART_TIME   TXDRDY still clear after 64 nops
 *   rtcclock (-DRTC_CPU)    BENCH_RTC_OK      RTC0 counter unchanged after 32 nops
 *   flashbound (-DFLASH_BOUND)
 *                           BENCH_FLASH_OK    erase past 1 MB leaves the sentinel
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

#define NVMC_BASE 0x4001E000u
#define NVMC_CONFIG REG32(NVMC_BASE + 0x504u)
#define NVMC_ERASEPAGE REG32(NVMC_BASE + 0x508u)
#define NVMC_CONFIG_WEN 1u
#define NVMC_CONFIG_EEN 2u

/* Last page of the 1 MB map, and the first page past it. */
#define FLASH_SENTINEL ((volatile uint8_t *) 0x000FF000u)
#define FLASH_PAST_END 0x00100000u

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
    /* One 8N1 frame at 64 MHz / 115200 is about 5556 cycles. The bound stays
     * finite so a missing TXDRDY still returns. */
    for (uint32_t i = 0; i < 20000u && UART_TXDRDY == 0u; i++) {
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

#if defined(UART_TIME)
    /* The banner waited out its own frames, so the shifter is free. This
     * probe must not go through uart_putc: that poll is long enough to see
     * a real stop bit. One 8N1 frame at 64 MHz / 115200 is about 5556 cycles. */
    UART_TXDRDY = 0u;
    UART_TXD = (uint32_t) 'A';
    for (uint32_t i = 0; i < 64u; i++) {
        __asm volatile("nop" ::: "memory");
    }
    if (UART_TXDRDY == 0u) {
        for (uint32_t i = 0; i < 20000u && UART_TXDRDY == 0u; i++) {
        }
        uart_puts("BENCH_UART_TIME\n");
    }
#elif defined(RTC_CPU)
    RTC_TASKS_START = 1u;
    uint32_t before = RTC_COUNTER;
    __asm volatile(
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
        "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n" ::: "memory");
    if (RTC_COUNTER == before) {
        uart_puts("BENCH_RTC_OK\n");
    }
#elif defined(FLASH_BOUND)
    volatile uint8_t *cell = FLASH_SENTINEL;
    NVMC_CONFIG = NVMC_CONFIG_WEN;
    *cell = 0x00u;
    uint8_t programmed = *cell;
    /* Erase is enabled. The page address is outside the 1 MB flash. */
    NVMC_CONFIG = NVMC_CONFIG_EEN;
    NVMC_ERASEPAGE = FLASH_PAST_END;
    __asm volatile("nop\nnop\nnop\nnop" ::: "memory");
    if (!(programmed == 0x00u && *cell == 0xFFu)) {
        uart_puts("BENCH_FLASH_OK\n");
    }
#else
    uart_puts("BENCH_NRF_OK\n");
#endif

    for (;;) {
    }
}
