//! Provide capsule driver for accessing an SD Card.
//! This allows initialization and block reads or writes on top of SPI

use core::cell::Cell;

use kernel::hil;
use kernel::common::take_cell::TakeCell;

#[allow(dead_code)]
#[derive(Clone,Copy,PartialEq)]
enum SDCmd {
    CMD0 = 0,
    CMD8 = 8,
    CMD55 = 55,
    ACMD41 = 0x80 + 41,
}

#[allow(dead_code)]
#[derive(Clone,Copy,PartialEq)]
enum SDResponse {
    R1,
    R2,
    R3,
    R7,
}

#[derive(Clone,Copy,PartialEq)]
enum State {
    Idle,

    SendACmd { acmd: SDCmd, arg: u32, response: SDResponse },

    InitDelayClocks,
    InitReset,
    InitCheckVersion,
    InitAppSpecificInitRepeat,
    InitAppSpecificInit,
    InitGenericInitRepeat,
    InitSetBlocksize,
}


//XXX: How do we handle errors with this interface?
pub trait SDCardClient {
    fn status(&self, status: u8);
    fn init_done(&self, status: u8);
    fn read_done(&self, data: &'static mut [u8], len: usize);
    fn write_done(&self, buffer: &'static mut [u8]);
}

// SD Card capsule, capable of being built on top of by other kernel capsules
pub struct SDCard<'a> {
    spi: &'a hil::spi::SPIMasterDevice,
    state: Cell<State>,
    after_state: Cell<State>,
    is_slow_mode: Cell<bool>,
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
            state: Cell::new(State::Idle),
            after_state: Cell::new(State::Idle),
            is_slow_mode: Cell::new(true),
            txbuffer: TakeCell::new(txbuffer),
            rxbuffer: TakeCell::new(rxbuffer),
            client: TakeCell::empty(),
        }
    }

    fn set_spi_slow_mode(&self) {
        // set to CPHA=0, CPOL=0, 400 kHZ
        self.spi.configure(hil::spi::ClockPolarity::IdleLow,
                           hil::spi::ClockPhase::SampleLeading,
                           400000);
    }

    fn set_spi_fast_mode(&self) {
        // set to CPHA=0, CPOL=0, 4 MHz
        self.spi.configure(hil::spi::ClockPolarity::IdleLow,
                           hil::spi::ClockPhase::SampleLeading,
                           4000000);
    }

    fn send_command(&self, cmd: SDCmd, arg: u32, response: SDResponse,
                    mut write_buffer: &'static mut [u8],
                    mut read_buffer: Option<&'static mut [u8]>) {
        if self.is_slow_mode.get() {
            self.set_spi_slow_mode();
        } else {
            self.set_spi_fast_mode();
        }

        // command
        write_buffer[0] = 0x40 | cmd as u8;

        // argument, MSB first
        write_buffer[1] = ((arg >> 24) & 0xFF) as u8;
        write_buffer[2] = ((arg >> 16) & 0xFF) as u8;
        write_buffer[3] = ((arg >>  8) & 0xFF) as u8;
        write_buffer[4] = ((arg >>  0) & 0xFF) as u8;

        // CRC is ignored except for CMD0 and maybe CMD8
        if cmd == SDCmd::CMD8 {
            write_buffer[5] = 0x87; // valid crc for CMD8(0x1AA)
        } else {
            write_buffer[5] = 0x95; // valid crc for CMD0
        }

        // calculate expected receive bytes
        let mut recv_len = match response {
            SDResponse::R1 => 1,
            SDResponse::R2 => 2,
            SDResponse::R3 => 5,
            SDResponse::R7 => 5,
        };

        // append dummy bytes to transmission
        for i in 0..recv_len {
            write_buffer[6+i] = 0xFF;
        }

        self.spi.read_write_bytes(write_buffer, read_buffer, 6+recv_len);
    }

    pub fn set_client<C: SDCardClient>(&self, client: &'static C) {
        self.client.replace(client);
    }

    //XXX: do we actually want an interrupt on install/uninstall?
    pub fn is_installed(&self) -> bool {
        // set initialized to be false if the card is ever gone
        true
    }

    pub fn initialize(&self) {
        // initially configure SPI to 400 khz
        self.set_spi_slow_mode();
        self.is_slow_mode.set(true);

        // delay for 80 clocks with CS asserted, allowing internal SD card
        //  state to initialize
        self.txbuffer.take().map(|txbuffer| {
            self.rxbuffer.take().map(move |rxbuffer| {
                self.state.set(State::InitDelayClocks);

                for i in 0..10 {
                    txbuffer[i] = 0xFF;
                }

                self.spi.read_write_bytes(txbuffer, Some(rxbuffer), 10);
            });
        });
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

        match self.state.get() {
            State::SendACmd { acmd, arg, response } => {
                // send the application-specific command and resume the state
                //  machine
                self.state.set(self.after_state.get());
                self.send_command(acmd, arg, response,
                    write_buffer, read_buffer);
            }

            State::InitDelayClocks => {
                // next reset SD card to idle state
                self.state.set(State::InitReset);
                self.send_command(SDCmd::CMD0, 0x0, SDResponse::R1,
                    write_buffer, read_buffer);
            }

            State::InitReset => {
                // check response
                //XXX: figure out how to do this
                self.rxbuffer.map(|rxbuffer| {
                    panic!("{:?}", rxbuffer);
                });

                // next send Check Voltage Range command that is only valid
                //  on SDv2 cards. This is used to check which SD card version
                //  is installed
                self.state.set(State::InitCheckVersion);
                self.send_command(SDCmd::CMD8, 0x1AA, SDResponse::R7,
                    write_buffer, read_buffer);
            }

            State::InitCheckVersion => {
                // check response
                // Branch based on the result

                    self.state.set(State::SendACmd { acmd: SDCmd::ACMD41, arg: 0x0, response: SDResponse::R1 });
                    self.after_state.set(State::InitAppSpecificInitRepeat);
                    self.send_command(SDCmd::CMD55, 0x0, SDResponse::R1,
                        write_buffer, read_buffer);
            }

            State::InitAppSpecificInitRepeat => {
            }

            State::InitAppSpecificInit => {
            }

            State::InitGenericInitRepeat => {
            }

            State::InitSetBlocksize => {
            }

            State::Idle => {}
        }
    }
}


// Application driver for SD Card capsule, layers on top of SD Card capsule
pub struct SDCardDriver<'a> {
    sdcard: &'a SDCard<'a>,
}

