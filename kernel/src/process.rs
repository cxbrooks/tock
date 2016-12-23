use callback::AppId;
use common::{RingBuffer, Queue, VolatileCell};

use container;
use core::{mem, ptr, slice};
use core::cell::Cell;
use core::intrinsics;
use core::ptr::{read_volatile, write_volatile};
use core::fmt::Write;

/// Takes a value and rounds it up to be aligned % 8
macro_rules! align8 {
    ( $e:expr ) => ( ($e) + ((8 - (($e) % 8)) % 8 ) );
}

#[no_mangle]
pub static mut SYSCALL_FIRED: usize = 0;

#[allow(improper_ctypes)]
extern "C" {
    pub fn switch_to_user(user_stack: *const u8,
                          mem_base: *const u8,
                          process_regs: &mut [usize; 8])
                          -> *mut u8;
}

pub static mut PROCS: &'static mut [Option<Process<'static>>] = &mut [];

pub fn schedule(callback: FunctionCall, appid: AppId) -> bool {
    let procs = unsafe { &mut PROCS };
    let idx = appid.idx();
    if idx >= procs.len() {
        return false;
    }

    match procs[idx] {
        None => false,
        Some(ref mut p) => {
            // TODO(alevy): validate appid liveness
            unsafe {
                HAVE_WORK.set(HAVE_WORK.get() + 1);
            }

            p.tasks.enqueue(Task::FunctionCall(callback))
        }
    }
}

#[derive(Copy,Clone,PartialEq,Eq)]
pub enum Error {
    NoSuchApp,
    OutOfMemory,
    AddressOutOfBounds,
}

#[derive(Copy,Clone,PartialEq,Eq)]
pub enum State {
    Running,
    Yielded,
}

#[derive(Copy, Clone)]
pub enum IPCType {
    Service,
    Client,
}

#[derive(Copy, Clone)]
pub enum Task {
    FunctionCall(FunctionCall),
    IPC((AppId, IPCType)),
}

#[derive(Copy, Clone)]
pub struct FunctionCall {
    pub r0: usize,
    pub r1: usize,
    pub r2: usize,
    pub r3: usize,
    pub pc: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct LoadInfo {
    version: u32,
    total_size: u32,
    entry_offset: u32,
    rel_data_offset: u32,
    rel_data_size: u32,
    text_offset: u32,
    text_size: u32,
    got_offset: u32,
    got_size: u32,
    data_offset: u32,
    data_size: u32,
    bss_mem_offset: u32,
    bss_size: u32,
    min_stack_len: u32,
    min_app_heap_len: u32,
    min_kernel_heap_len: u32,
    pkg_name_offset: u32,
    pkg_name_size: u32,
    checksum: u32,
}

/// Converts a pointer to memory to a LoadInfo struct
///
/// This function takes a pointer to arbitrary memory and Optionally returns a
/// LoadInfo struct. This function will validate the header checksum, but does
/// not perform sanity or security checking on the structure
unsafe fn parse_and_validate_load_info(address: *const u8) -> Option<&'static LoadInfo> {
    let load_info = &*(address as *const LoadInfo);

    if load_info.version != 1 {
        return None;
    }

    let checksum =
        load_info.version ^ load_info.total_size ^ load_info.entry_offset ^
        load_info.rel_data_offset ^ load_info.rel_data_size ^ load_info.text_offset ^
        load_info.text_size ^ load_info.got_offset ^
        load_info.got_size ^ load_info.data_offset ^ load_info.data_size ^
        load_info.bss_mem_offset ^ load_info.bss_size ^ load_info.min_stack_len ^
        load_info.min_app_heap_len ^
        load_info.min_kernel_heap_len ^ load_info.pkg_name_offset ^ load_info.pkg_name_size;

    if checksum != load_info.checksum {
        return None;
    }

    Some(load_info)
}

pub struct Process<'a> {
    /// Application memory layout:
    ///
    ///  |======== <- memory[memory.len()]
    ///  | Grant
    ///  |   ↓
    ///  |  ----   <- kernel_memory_break
    ///  |
    ///  |  ----   <- app_memory_break
    ///  |   ↑
    ///  |  Heap
    ///  |  ----   <- stack_heap_boundary
    ///  | Stack
    ///  |   ↓
    ///  |  ----   <- cur_stack
    ///  |
    ///  |  ----   <- app_mem_start
    ///  |  Data
    ///  |======== <- memory[0]

    /// The process's memory.
    memory: &'static mut [u8],

    kernel_memory_break: *const u8,
    app_memory_break: *const u8,
    stack_heap_boundary: *const u8,
    cur_stack: *const u8,
    app_mem_start: *const u8,

    /// Process text segment
    text: &'static [u8],

    /// The beginning of process code after the LoadInfo and RelData sections.
    /// Note that unlike most pointers, this is to the flash image, not loaded sram
    app_flash_code_start: *const u8,

    stored_regs: [usize; 8],

    yield_pc: usize,
    psr: usize,

    state: State,

    /// MPU regions are saved as a pointer-size pair.
    ///
    /// size is encoded as X where
    /// SIZE = 2^(X + 1) and X >= 4.
    ///
    /// A null pointer represents an empty region.
    ///
    /// # Invariants
    ///
    /// The pointer must be aligned to the size. E.g. if the size is 32 bytes, the pointer must be
    /// 32-byte aligned.
    ///
    mpu_regions: [Cell<(*const u8, usize)>; 5],

    tasks: RingBuffer<'a, Task>,

    pub package_name: &'static [u8],
}

fn closest_power_of_two(mut num: u32) -> u32 {
    num -= 1;
    num |= num >> 1;
    num |= num >> 2;
    num |= num >> 4;
    num |= num >> 8;
    num |= num >> 16;
    num += 1;
    num
}

// Stores the current number of callbacks enqueued + processes in Running state
static mut HAVE_WORK: VolatileCell<usize> = VolatileCell::new(0);

pub fn processes_blocked() -> bool {
    unsafe { HAVE_WORK.get() == 0 }
}

impl<'a> Process<'a> {
    pub fn schedule_ipc(&mut self, from: AppId, cb_type: IPCType) {
        unsafe {
            HAVE_WORK.set(HAVE_WORK.get() + 1);
        }
        self.tasks.enqueue(Task::IPC((from, cb_type)));
    }

    pub fn current_state(&self) -> State {
        self.state
    }

    pub fn yield_state(&mut self) {
        if self.state == State::Running {
            self.state = State::Yielded;
            unsafe {
                HAVE_WORK.set(HAVE_WORK.get() - 1);
            }
        }
    }

    pub fn dequeue_task(&mut self) -> Option<Task> {
        self.tasks.dequeue().map(|cb| {
            unsafe {
                HAVE_WORK.set(HAVE_WORK.get() - 1);
            }
            cb
        })
    }

    pub fn mem_start(&self) -> *const u8 {
        self.memory.as_ptr()
    }

    pub fn mem_end(&self) -> *const u8 {
        unsafe { self.memory.as_ptr().offset(self.memory.len() as isize) }
    }

    pub fn setup_mpu(&self, mpu: &::platform::MPU) {
        let data_start = self.memory.as_ptr() as usize;
        let data_len = (32 - self.memory.len().leading_zeros()) as u32;

        let text_start = self.text.as_ptr() as usize;
        let text_len = ((32 - self.text.len().leading_zeros()) - 2) as u32;

        let mut grant_size = unsafe {
            self.memory.as_ptr().offset(self.memory.len() as isize) as u32 -
            (self.kernel_memory_break as u32)
        };
        grant_size = closest_power_of_two(grant_size);
        let grant_base = unsafe {
            self.memory
                .as_ptr()
                .offset(self.memory.len() as isize)
                .offset(-(grant_size as isize))
        };
        let mgrant_size = grant_size.trailing_zeros() - 1;

        // Data segment read/write/execute
        mpu.set_mpu(0, data_start as u32, data_len, true, 0b011);
        // Text segment read/execute (no write)
        mpu.set_mpu(1, text_start as u32, text_len, true, 0b111);

        // Disallow access to grant region
        mpu.set_mpu(2, grant_base as u32, mgrant_size, false, 0b001);

        for (i, region) in self.mpu_regions.iter().enumerate() {
            mpu.set_mpu((i + 3) as u32,
                        region.get().0 as u32,
                        region.get().1 as u32,
                        true,
                        0b011);
        }
    }


    pub fn add_mpu_region(&self, base: *const u8, size: usize) -> bool {
        if size >= 16 && size.count_ones() == 1 && (base as usize) % size == 0 {
            let mpu_size = (size.trailing_zeros() - 1) as usize;
            for region in self.mpu_regions.iter() {
                if region.get().0 == ptr::null() {
                    region.set((base, mpu_size));
                    return true;
                } else if region.get().0 == base {
                    if region.get().1 < mpu_size {
                        region.set((base, mpu_size));
                    }
                    return true;
                }
            }
        }
        return false;
    }

    pub unsafe fn create(app_flash_address: *const u8,
                         remaining_app_memory: *mut u8,
                         remaining_app_memory_size: usize)
                         -> (Option<Process<'a>>, usize, usize) {
        if let Some(load_info) = parse_and_validate_load_info(app_flash_address) {
            let app_flash_size = load_info.total_size as usize;

            // Load the process into memory
            if let Some(load_result) =
                load(load_info,
                     app_flash_address,
                     remaining_app_memory,
                     remaining_app_memory_size) {
                let stack_len = align8!(load_info.min_stack_len);
                let app_heap_len = align8!(load_info.min_app_heap_len);
                let kernel_heap_len = align8!(load_info.min_kernel_heap_len);

                let app_slice_size =
                    (load_result.data_len + stack_len + app_heap_len + kernel_heap_len) as usize;
                // TODO round app_slice_size up to MPU unit?

                if app_slice_size > remaining_app_memory_size {
                    panic!("{:?} failed to load. Insufficient memory. Requested {} have {}",
                           load_result.package_name,
                           app_slice_size,
                           remaining_app_memory_size);
                }

                let app_memory = slice::from_raw_parts_mut(remaining_app_memory, app_slice_size);
                let stack_heap_boundary = app_memory.as_mut_ptr()
                    .offset((load_result.data_len + stack_len) as isize);
                let app_memory_break = stack_heap_boundary;

                // Set up initial grant region
                let mut kernel_memory_break = app_memory.as_mut_ptr()
                    .offset(app_memory.len() as isize);

                // make room for container pointers
                let pointer_size = mem::size_of::<*const usize>();
                let num_ctrs = read_volatile(&container::CONTAINER_COUNTER);
                let container_ptrs_size = num_ctrs * pointer_size;
                kernel_memory_break = kernel_memory_break.offset(-(container_ptrs_size as isize));

                // set all pointers to null
                let opts = slice::from_raw_parts_mut(kernel_memory_break as *mut *const usize,
                                                     num_ctrs);
                for opt in opts.iter_mut() {
                    *opt = ptr::null()
                }

                // Allocate memory for callback ring buffer
                let callback_size = mem::size_of::<Task>();
                let callback_len = 10;
                let callback_offset = callback_len * callback_size;
                kernel_memory_break = kernel_memory_break.offset(-(callback_offset as isize));

                // Set up ring buffer
                let callback_buf = slice::from_raw_parts_mut(kernel_memory_break as *mut Task,
                                                             callback_len);
                let tasks = RingBuffer::new(callback_buf);

                let mut process = Process {
                    memory: app_memory,

                    kernel_memory_break: kernel_memory_break,
                    app_memory_break: app_memory_break,
                    stack_heap_boundary: stack_heap_boundary,
                    cur_stack: stack_heap_boundary,
                    app_mem_start: load_result.app_mem_start,

                    text: slice::from_raw_parts(app_flash_address, app_flash_size),

                    app_flash_code_start: load_result.app_flash_code_start,

                    stored_regs: [0; 8],
                    yield_pc: load_result.init_fn,
                    // Set the Thumb bit and clear everything else
                    psr: 0x01000000,

                    state: State::Yielded,

                    mpu_regions: [Cell::new((ptr::null(), 0)),
                                  Cell::new((ptr::null(), 0)),
                                  Cell::new((ptr::null(), 0)),
                                  Cell::new((ptr::null(), 0)),
                                  Cell::new((ptr::null(), 0))],
                    tasks: tasks,
                    package_name: load_result.package_name,
                };

                if (load_result.init_fn & 0x1) != 1 {
                    panic!("{:?} process image invalid. \
                           init_fn address must end in 1 to be Thumb, got {:#X}",
                           load_result.package_name,
                           load_result.init_fn);
                }

                process.tasks.enqueue(Task::FunctionCall(FunctionCall {
                    pc: load_result.init_fn,
                    r0: load_result.app_mem_start as usize,
                    r1: process.app_memory_break as usize,
                    r2: process.kernel_memory_break as usize,
                    r3: 0,
                }));

                HAVE_WORK.set(HAVE_WORK.get() + 1);

                return (Some(process), app_flash_size, app_slice_size);
            }
        }
        (None, 0, 0)
    }

    pub fn sbrk(&mut self, increment: isize) -> Result<*const u8, Error> {
        let new_break = unsafe { self.app_memory_break.offset(increment) };
        self.brk(new_break)
    }

    pub fn brk(&mut self, new_break: *const u8) -> Result<*const u8, Error> {
        if new_break < self.mem_start() || new_break >= self.mem_end() {
            Err(Error::AddressOutOfBounds)
        } else if new_break > self.kernel_memory_break {
            Err(Error::OutOfMemory)
        } else {
            let old_break = self.app_memory_break;
            self.app_memory_break = new_break;
            Ok(old_break)
        }
    }

    pub fn in_exposed_bounds(&self, buf_start_addr: *const u8, size: usize) -> bool {

        let buf_end_addr = unsafe { buf_start_addr.offset(size as isize) };

        buf_start_addr >= self.mem_start() && buf_end_addr <= self.mem_end()
    }

    pub unsafe fn alloc(&mut self, size: usize) -> Option<&mut [u8]> {
        let new_break = self.kernel_memory_break.offset(-(size as isize));
        if new_break < self.app_memory_break {
            None
        } else {
            self.kernel_memory_break = new_break;
            Some(slice::from_raw_parts_mut(new_break as *mut u8, size))
        }
    }

    pub unsafe fn free<T>(&mut self, _: *mut T) {}

    pub unsafe fn container_for<T>(&mut self, container_num: usize) -> *mut *mut T {
        let container_num = container_num as isize;
        let ptr = (self.mem_end() as *mut usize).offset(-(container_num + 1));
        ptr as *mut *mut T
    }

    pub unsafe fn container_for_or_alloc<T: Default>(&mut self,
                                                     container_num: usize)
                                                     -> Option<*mut T> {
        let ctr_ptr = self.container_for::<T>(container_num);
        if (*ctr_ptr).is_null() {
            self.alloc(mem::size_of::<T>()).map(|root_arr| {
                let root_ptr = root_arr.as_mut_ptr() as *mut T;
                *root_ptr = Default::default();
                write_volatile(ctr_ptr, root_ptr);
                root_ptr
            })
        } else {
            Some(*ctr_ptr)
        }
    }


    pub fn pop_syscall_stack(&mut self) {
        let pspr = self.cur_stack as *const usize;
        unsafe {
            self.yield_pc = read_volatile(pspr.offset(6));
            self.psr = read_volatile(pspr.offset(7));
            self.cur_stack = (self.cur_stack as *mut usize).offset(8) as *mut u8;
        }
    }

    /// Context switch to the process.
    pub unsafe fn push_function_call(&mut self, callback: FunctionCall) {
        HAVE_WORK.set(HAVE_WORK.get() + 1);

        self.state = State::Running;
        // Fill in initial stack expected by SVC handler
        // Top minus 8 u32s for r0-r3, r12, lr, pc and xPSR
        let stack_bottom = (self.cur_stack as *mut usize).offset(-8);
        write_volatile(stack_bottom.offset(7), self.psr);
        write_volatile(stack_bottom.offset(6), callback.pc | 1);
        // Set the LR register to the saved PC so the callback returns to
        // wherever wait was called. Set lowest bit to one because of THUMB
        // instruction requirements.
        write_volatile(stack_bottom.offset(5), self.yield_pc | 0x1);
        write_volatile(stack_bottom, callback.r0);
        write_volatile(stack_bottom.offset(1), callback.r1);
        write_volatile(stack_bottom.offset(2), callback.r2);
        write_volatile(stack_bottom.offset(3), callback.r3);

        self.cur_stack = stack_bottom as *mut u8;
    }

    pub unsafe fn syscall_fired(&self) -> bool {
        read_volatile(&SYSCALL_FIRED) != 0
    }

    /// Context switch to the process.
    pub unsafe fn switch_to(&mut self) {
        write_volatile(&mut SYSCALL_FIRED, 0);
        let psp = switch_to_user(self.cur_stack, self.memory.as_ptr(), &mut self.stored_regs);
        self.cur_stack = psp;
    }

    pub fn svc_number(&self) -> Option<u8> {
        let psp = self.cur_stack as *const *const u16;
        unsafe {
            let pcptr = read_volatile((psp as *const *const u16).offset(6));
            let svc_instr = read_volatile(pcptr.offset(-1));
            Some((svc_instr & 0xff) as u8)
        }
    }

    pub fn lr(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(5)) }
    }


    pub fn r0(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr) }
    }

    pub fn set_r0(&mut self, val: isize) {
        let pspr = self.cur_stack as *mut isize;
        unsafe { write_volatile(pspr, val) }
    }

    pub fn r1(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(1)) }
    }

    pub fn r2(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(2)) }
    }

    pub fn r3(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(3)) }
    }

    pub unsafe fn statistics_str<W: Write>(&self, writer: &mut W) {

        // determine app state
        //  the actual app PC depends on whether it has yielded or is still
        //  "running" (i.e. in a syscall)
        let mut state_str = "Yielded";
        let mut pc_addr = self.yield_pc;
        if self.state == State::Running {
            state_str = "Running";
            pc_addr = self.lr() as usize;
        }

        // memory region
        let mem_end = self.mem_end() as usize;
        let mem_start = self.mem_start() as usize;

        // grant region size
        let grant_start = self.kernel_memory_break as usize;
        let grant_size = mem_end - grant_start;

        // unallocated space
        let heap_top = self.app_memory_break as usize;
        let unallocated_size = grant_start - heap_top;

        // heap plus stack region
        let stack_bottom = self.cur_stack as usize;
        let heapstack_size = heap_top - stack_bottom;

        // remaining stack space
        let data_end = self.app_mem_start as usize;
        let stack_remaining_size = stack_bottom - data_end;

        // static data region (GOT, Data, and BSS)
        let static_data_size = data_end - mem_start;

        // text region
        let text_start = self.text.as_ptr() as usize;
        let text_end = self.text.as_ptr().offset(self.text.len() as isize) as usize;
        let text_size = text_end - text_start;

        // PC address in app lst file
        let pc_addr_relative = 0x80000000 | (0xFFFFFFFE & (pc_addr - self.app_flash_code_start as usize));

        // number of events queued
        let events_queued = self.tasks.len();

        let _ = writer.write_fmt(format_args!("\
        [{}]  -  Events Queued: {}\
          \r\n  {:#010X} |========\
        \r\n             | Grant       [{:5} bytes]\
          \r\n  {:#010X} | ---\
        \r\n             | Unallocated [{:5} bytes]\
          \r\n  {:#010X} | ---\
        \r\n             | Heap+Stack  [{:5} bytes]\
          \r\n  {:#010X} | ---\
        \r\n             | Unallocated [{:5} bytes]\
          \r\n  {:#010X} | ---\
        \r\n             | Data        [{:5} bytes]\
          \r\n  {:#010X} |========\
        \r\n             .....\
          \r\n  {:#010X} |========\
        \r\n             | Text        [{:5} bytes]\
          \r\n  {:#010X} |========\
        \r\n\
          \r\n  PC: {:#010X} [{:#010X} in lst file]\
        \r\n\r\n", state_str, events_queued, mem_end, grant_size, grant_start,
        unallocated_size, heap_top, heapstack_size, stack_bottom,
        stack_remaining_size, data_end, static_data_size, mem_start, text_end,
        text_size, text_start, pc_addr, pc_addr_relative));
    }
}

#[derive(Debug)]
struct LoadResult {
    /// The absolute address of the process entry point (i.e. `_start`).
    init_fn: usize,

    /// The lowest free address in process memory after loading the GOT, data
    /// and BSS.
    app_mem_start: *const u8,

    /// The length of the data segment
    data_len: u32,

    /// The beginning of process code in Flash after LoadInfo and RelData
    app_flash_code_start: *const u8,

    /// The process's package name (used for IPC)
    package_name: &'static [u8],
}

/// Loads the process into memory
///
/// Loads the process whos binary starts at `flash_start_addr` into the memory
/// region beginning at `mem_base`. The process binary must fit within
/// `mem_size` bytes.
///
/// This function will copy the GOT and data segment into memory as well as
/// zero out the BSS section. It performs relocation on the GOT and on
/// variables named in the relocation section of the binary.
///
/// The function returns a `LoadResult` containing metadata about the loaded
/// process or None if loading failed.
unsafe fn load(load_info: &'static LoadInfo,
               flash_start_addr: *const u8,
               mem_base: *mut u8,
               mem_size: usize)
               -> Option<LoadResult> {
    // unsafe fn parse_and_validate_load_info(address: *const u8) -> Option<&'static LoadInfo> {
    let mem_end = mem_base.offset(mem_size as isize);

    let mut load_result = LoadResult {
        package_name:
            slice::from_raw_parts(flash_start_addr.offset(load_info.pkg_name_offset as isize),
                                  load_info.pkg_name_size as usize),
        init_fn: 0,
        app_mem_start: ptr::null(),
        data_len: 0,
        app_flash_code_start: ptr::null(),
    };

    let text_start = flash_start_addr.offset(load_info.text_offset as isize);
    load_result.app_flash_code_start = text_start;

    let rel_data: &[u32] =
        slice::from_raw_parts(flash_start_addr.offset(
                load_info.rel_data_offset as isize) as *const u32,
                (load_info.rel_data_size as usize) / mem::size_of::<u32>());

    let got: &[u8] =
        slice::from_raw_parts(flash_start_addr.offset(load_info.got_offset as isize),
                              load_info.got_size as usize) as &[u8];

    let data: &[u8] =
        slice::from_raw_parts(flash_start_addr.offset(load_info.data_offset as isize),
                              load_info.data_size as usize);

    let target_data: &mut [u8] =
        slice::from_raw_parts_mut(mem_base,
                                  (load_info.data_size + load_info.got_size) as usize);

    // Verify target data fits in memory
    if target_data.len() > mem_size {
        // When a kernel warning mechanism exists, this panic should be
        // replaced with that, but for now it seems more useful to bail out to
        // alert developers of why the app failed to load
        panic!("{:?} failed to load. Data + GOT ({}) exceeded available memory ({})",
               load_result.package_name,
               target_data.len(),
               mem_size);
    }

    // Copy the GOT and data into base memory
    for (orig, dest) in got.iter().chain(data.iter()).zip(target_data.iter_mut()) {
        *dest = *orig
    }

    // Zero out BSS
    let bss = mem_base.offset(load_info.bss_mem_offset as isize);
    if bss.offset(load_info.bss_size as isize) > mem_end {
        panic!("{:?} failed to load. BSS overran available memory",
               load_result.package_name);
    }
    intrinsics::write_bytes(mem_base.offset(load_info.bss_mem_offset as isize),
                            0,
                            load_info.bss_size as usize);


    let fixup = |addr: &mut u32| {
        let entry = *addr;
        if (entry & 0x80000000) == 0 {
            // Regular data (memory relative)
            *addr = entry + (mem_base as u32);
        } else {
            // rodata or function pointer (code relative)
            *addr = (entry ^ 0x80000000) + (text_start as u32);
        }
    };

    // Fixup Global Offset Table
    // No need to validate size here, covered by target_data check above
    let mem_got: &mut [u32] = slice::from_raw_parts_mut(mem_base as *mut u32,
                                                        (load_info.got_size as usize) /
                                                        mem::size_of::<u32>());

    for got_cur in mem_got {
        fixup(got_cur);
    }

    // Fixup relocation data
    for (i, addr) in rel_data.iter().enumerate() {
        if i % 2 == 0 {
            // Only the first of every 2 entries is an address
            fixup(&mut *(mem_base.offset(*addr as isize) as *mut u32));
        }
    }

    // Entry point is offset from app code
    load_result.init_fn = flash_start_addr.offset(load_info.entry_offset as isize) as usize;

    let aligned_mem_start = align8!(load_info.bss_mem_offset + load_info.bss_size);
    load_result.app_mem_start = mem_base.offset(aligned_mem_start as isize);
    load_result.data_len = aligned_mem_start;

    Some(load_result)
}
