//! The SA-1 register file (`$2200-$230E`), decoded. Field names follow
//! fullsnes; the `*fl`/`*cl`/`*en` triplets are ares' interrupt model: a
//! flag is what the status ports show, an enable gates delivery, and a
//! clear latch is false while an interrupt waits to be taken.

/// Every writable register plus the status latches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Io {
    // $2200 CCNT
    /// IRQ from the S-CPU to the SA-1.
    pub(crate) sa1_irq: bool,
    /// SA-1 wait (bit 6).
    pub(crate) sa1_rdyb: bool,
    /// SA-1 reset (bit 5).
    pub(crate) sa1_resb: bool,
    /// NMI from the S-CPU to the SA-1.
    pub(crate) sa1_nmi: bool,
    /// Message S-CPU → SA-1.
    pub(crate) smeg: u8,
    // $2201 SIE / $2202 SIC
    pub(crate) cpu_irqen: bool,
    pub(crate) chdma_irqen: bool,
    pub(crate) cpu_irqcl: bool,
    pub(crate) chdma_irqcl: bool,
    // $2203-$2208 vectors for the SA-1
    pub(crate) crv: u16,
    pub(crate) cnv: u16,
    pub(crate) civ: u16,
    // $2209 SCNT
    pub(crate) cpu_irq: bool,
    pub(crate) cpu_ivsw: bool,
    pub(crate) cpu_nvsw: bool,
    /// Message SA-1 → S-CPU.
    pub(crate) cmeg: u8,
    // $220A CIE / $220B CIC
    pub(crate) sa1_irqen: bool,
    pub(crate) timer_irqen: bool,
    pub(crate) dma_irqen: bool,
    pub(crate) sa1_nmien: bool,
    pub(crate) sa1_irqcl: bool,
    pub(crate) timer_irqcl: bool,
    pub(crate) dma_irqcl: bool,
    pub(crate) sa1_nmicl: bool,
    // $220C-$220F vectors for the S-CPU
    pub(crate) snv: u16,
    pub(crate) siv: u16,
    // $2210 TMC, $2212-$2215
    pub(crate) hvselb: bool,
    pub(crate) ven: bool,
    pub(crate) hen: bool,
    pub(crate) hcnt: u16,
    pub(crate) vcnt: u16,
    // $2220-$2223 Super MMC: (bank, LoROM mode) for blocks C-F
    pub(crate) mmc: [(u8, bool); 4],
    // $2224 BMAPS, $2225 BMAP
    pub(crate) sbm: u8,
    pub(crate) sw46: bool,
    pub(crate) cbm: u8,
    // $2226 SBWE, $2227 CBWE, $2228 BWPA, $2229 SIWP, $222A CIWP
    pub(crate) swen: bool,
    pub(crate) cwen: bool,
    pub(crate) bwp: u8,
    pub(crate) siwp: u8,
    pub(crate) ciwp: u8,
    // $2230 DCNT
    pub(crate) dmaen: bool,
    pub(crate) dprio: bool,
    pub(crate) cden: bool,
    pub(crate) cdsel: bool,
    pub(crate) dd: bool,
    pub(crate) sd: u8,
    // $2231 CDMA
    pub(crate) chdend: bool,
    pub(crate) dmasize: u8,
    pub(crate) dmacb: u8,
    // $2232-$2239
    pub(crate) dsa: u32,
    pub(crate) dda: u32,
    pub(crate) dtc: u16,
    // $223F BBF, $2240-$224F BRF
    pub(crate) bbf: bool,
    pub(crate) brf: [u8; 16],
    // $2250 MCNT, $2251-$2254
    pub(crate) acm: bool,
    pub(crate) md: bool,
    pub(crate) ma: u16,
    pub(crate) mb: u16,
    // $2258 VBD, $2259-$225B VDA
    pub(crate) hl: bool,
    pub(crate) vb: u8,
    pub(crate) va: u32,
    pub(crate) vbit: u8,
    // $2300 SFR
    pub(crate) cpu_irqfl: bool,
    pub(crate) chdma_irqfl: bool,
    // $2301 CFR
    pub(crate) sa1_irqfl: bool,
    pub(crate) timer_irqfl: bool,
    pub(crate) dma_irqfl: bool,
    pub(crate) sa1_nmifl: bool,
    // $2302-$2305 latched counters
    pub(crate) hcr: u16,
    pub(crate) vcr: u16,
    // $2306-$230B
    pub(crate) mr: u64,
    pub(crate) overflow: bool,
    /// Character conversion 1 is feeding the S-CPU's BW-RAM reads.
    pub(crate) bwram_dma: bool,
}

impl Default for Io {
    fn default() -> Self {
        Self::new()
    }
}

impl Io {
    /// Power-on values (fullsnes "Reset": `CCNT = $20`, `BWPA = $FF`,
    /// MMC banks 0-3, everything else zero).
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            sa1_irq: false,
            sa1_rdyb: false,
            sa1_resb: true,
            sa1_nmi: false,
            smeg: 0,
            cpu_irqen: false,
            chdma_irqen: false,
            cpu_irqcl: false,
            chdma_irqcl: false,
            crv: 0,
            cnv: 0,
            civ: 0,
            cpu_irq: false,
            cpu_ivsw: false,
            cpu_nvsw: false,
            cmeg: 0,
            sa1_irqen: false,
            timer_irqen: false,
            dma_irqen: false,
            sa1_nmien: false,
            sa1_irqcl: false,
            timer_irqcl: false,
            dma_irqcl: false,
            sa1_nmicl: false,
            snv: 0,
            siv: 0,
            hvselb: false,
            ven: false,
            hen: false,
            hcnt: 0,
            vcnt: 0,
            mmc: [(0, false), (1, false), (2, false), (3, false)],
            sbm: 0,
            sw46: false,
            cbm: 0,
            swen: false,
            cwen: false,
            bwp: 0x0F,
            siwp: 0,
            ciwp: 0,
            dmaen: false,
            dprio: false,
            cden: false,
            cdsel: false,
            dd: false,
            sd: 0,
            chdend: false,
            dmasize: 0,
            dmacb: 0,
            dsa: 0,
            dda: 0,
            dtc: 0,
            bbf: false,
            brf: [0; 16],
            acm: false,
            md: false,
            ma: 0,
            mb: 0,
            hl: false,
            vb: 16,
            va: 0,
            vbit: 0,
            cpu_irqfl: false,
            chdma_irqfl: false,
            sa1_irqfl: false,
            timer_irqfl: false,
            dma_irqfl: false,
            sa1_nmifl: false,
            hcr: 0,
            vcr: 0,
            mr: 0,
            overflow: false,
            bwram_dma: false,
        }
    }
}
