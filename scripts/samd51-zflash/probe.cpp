// SPDX-License-Identifier: MIT
// Authored caller of the actual separately source-bound ZFlash implementation.
#include "pxt-flash-platform.h"
#include "Flash.h"
extern "C" {
volatile unsigned zflash_witness[16];
void *memset(void *p,int value,size_t n) { auto *b=(unsigned char*)p; for(size_t i=0;i<n;i++) b[i]=(unsigned char)value; return p; }
void *memcpy(void *p,const void *q,size_t n) { auto *b=(unsigned char*)p;auto *a=(const unsigned char*)q;for(size_t i=0;i<n;i++)b[i]=a[i];return p; }
void target_panic(int reason) { zflash_witness[14]=(unsigned)reason;for(;;)__asm__ volatile("nop"); }
void fault() {target_panic(0xf00d);}
void __cxa_pure_virtual() {target_panic(0xbad);}
// Real CPS instructions, a nonnested boot-only caller, not CODAL fiber support.
void target_disable_irq() {__asm__ volatile("cpsid i":::"memory");zflash_witness[12]++;}
void target_enable_irq() {zflash_witness[13]++;__asm__ volatile("cpsie i":::"memory");}
__attribute__((noinline)) void zflash_checkpoint(unsigned stage) { zflash_witness[15]=stage; __asm__ volatile("nop":::"memory"); }
void zflash_probe() {
    codal::ZFlash flash;
    alignas(8) unsigned char source[32];
    for(unsigned i=0;i<32;i++)source[i]=(unsigned char)(i*7+3);
    zflash_witness[1]=flash.codal::ZFlash::totalSize();
    zflash_witness[2]=flash.codal::ZFlash::pageSize(0x78000);
    zflash_checkpoint(1);
    zflash_witness[3]=flash.codal::ZFlash::erasePage(0x78000);
    zflash_checkpoint(2);
    zflash_witness[4]=flash.codal::ZFlash::writeBytes(0x78000,source,32);
    zflash_checkpoint(3);
    zflash_witness[5]=flash.codal::ZFlash::writeBytes(0x78028,source,8);
    zflash_checkpoint(4);
    zflash_witness[6]=flash.codal::ZFlash::writeBytes(0x78040,source,7);
    zflash_witness[7]=flash.codal::ZFlash::writeBytes(0x78041,source,8);
    zflash_witness[8]=flash.codal::ZFlash::writeBytes(0x78000,source,8);
    zflash_checkpoint(5);
    zflash_witness[10]=flash.codal::ZFlash::erasePage(0x79000);
    zflash_checkpoint(6);
    zflash_witness[0]=0x600d;
    for(;;)__asm__ volatile("nop");
}
}
