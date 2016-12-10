//! Provide capsule driver for accessing an SD Card.
//! This allows initialization and block reads or writes on top of SPI

use kernel::hil;


//XXX: How do we handle errors with this interface?
pub trait SDCardClient {
    fn status(&self, status: u8);
    fn read_done(&self, data: &'static mut [u8], len: usize);
    fn write_done(&self, buffer: &'static mut [u8]);
}

// SD Card capsule, capable of being built on top of by other kernel capsules
pub struct SDCard<'a> {
    spi: &'a hil::spi::SPIMasterDevice,
    txbuffer: TakeCell<&'static mut [u8]>,
    rxbuffer: TakeCell<&'static mut [u8]>,
    client: TakeCell<&'static SDCardClient>,
}

impl<'a> SDCard<'a> {
    pub fn new(spi: &'a hil::spi::SPIMasterDevice,
               txbuffer: &'static mut [u8],
               rxbuffer: &'static mut [u8])
               -> SDCard<'a> {
        // setup and return struct
        SDCard {
            spi: spi,
            txbuffer: TakeCell::new(txbuffer),
            rxbuffer: TakeCell::new(rxbuffer),
            client: TakeCell::empty(),
        }
    }

    pub fn set_client<C: SDCardClient>(&self, client: &'static C) {
        self.client.replace(client);
    }

    //XXX: do we actually want an interrupt on install/uninstall?
    pub fn is_installed(&self) -> bool {
        // set initialized to be false if the card is ever gone
    }

    pub fn initialize(&self) {
        // initially configure SPI to 400 khz
        
        // ~80 clocks worth of delay after asserting CS
        //  apparently can be done by transmitting 0xFFs?
        //  just do a 10-byte write at 400 khz
        
        // CMD0 - reset to idle state
        
        //XXX: do we actually care about SDv2, SDv1, and MMC?
        // CMD8 - checks if SDv2

            // ACMD41 - application-specific initialize
            
            // repeat until not idle
        
        // else SDv1 or MMC

            // ACMD41 - application-specific initialize
            
            // CMD1 - generic initialize
            
            // repeat until not idle
            
            // CMD16 - set blocksize to 512
    }

    pub fn read_block(&self) {
        // only if initialzed and installed
    }

    pub fn write_block(&self) {
        // only if intialized and installed
    }
}

impl<'a> hil::spi::SpiMasterClient for SDCard<'a> {
    fn read_write_done(&self,
                       mut write_buffer: &'static mut [u8],
                       mut read_buffer: Option<&'static mut [u8]>,
                       len: usize) {
    }
}


// Application driver for SD Card capsule, layers on top of SD Card capsule
pub struct SDCardDriver<'a> {
    sdcard: &'a SDCard<'a>,
}

