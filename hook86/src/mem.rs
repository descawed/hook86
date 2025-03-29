use std::ffi::c_void;
use std::collections::HashMap;

use memchr::memmem;
use windows::core::{PWSTR, Result};
use windows::Win32::Foundation::{HMODULE, MAX_PATH};
use windows::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, PAGE_READWRITE, PAGE_WRITECOPY};
use windows::Win32::System::ProcessStatus::{
    EnumProcessModules, GetModuleBaseNameW, GetModuleInformation, MODULEINFO,
};
use windows::Win32::System::Threading::GetCurrentProcess;

pub use hook86_core::{unprotect, protect, patch};

/// A utility for searching for byte strings in memory
///
/// The ByteSearcher can search for multiple strings at one time. Searches can be filtered by the
/// protection level of the memory region and/or the module that the memory region was loaded from.
/// Filtering by module requires first calling the discover_modules() method to enumerate the
/// modules loaded in the current process.
#[derive(Debug)]
pub struct ByteSearcher {
    modules: HashMap<String, (*const c_void, *const c_void)>,
}

impl ByteSearcher {
    /// Create a new ByteSearcher
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    fn search_in_ranges<'a, T: Default + Copy, const N: usize>(
        protection: Option<PAGE_PROTECTION_FLAGS>,
        ranges: impl Iterator<Item = &'a (*const c_void, *const c_void)>,
        search_func: impl Fn(*const u8, usize, &mut [T]) -> bool,
    ) -> [T; N] {
        let mut results = [Default::default(); N];
        for &(start, end) in ranges {
            let mut addr = start;
            while addr < end {
                let mut memory_info = MEMORY_BASIC_INFORMATION::default();
                let result = unsafe {
                    VirtualQuery(Some(addr), &mut memory_info, size_of_val(&memory_info))
                };
                if result == 0 {
                    break;
                }

                let search_base = addr as *const u8;
                addr = unsafe { memory_info.BaseAddress.add(memory_info.RegionSize) };

                if memory_info.State != MEM_COMMIT
                    || protection.is_some_and(|p| !p.contains(memory_info.Protect))
                {
                    continue;
                }

                if search_func(search_base, memory_info.RegionSize, &mut results) {
                    // if search_func returns true, we've found everything we were looking for
                    break;
                }
            }
        }

        results
    }

    /// Search for byte strings in a range of addresses
    ///
    /// # Arguments
    ///
    /// * `patterns` - The byte strings to search for
    /// * `protection` - If provided, only search memory regions matching one of the specified protection flags
    /// * `ranges` - An iterator of (start, end) address tuples defining the address ranges to search
    ///
    /// # Return
    ///
    /// An array of `Option<*const c_void>` with the same number of elements as the `patterns` argument.
    /// If the corresponding byte string was found, the value will be `Some(ptr)`, where `ptr` is a
    /// pointer to the location where the byte string was found. If the byte string was not found,
    /// the element in the return array will be `None`.
    pub fn find_bytes_in_ranges<'a, const N: usize>(
        patterns: &[&[u8]; N],
        protection: Option<PAGE_PROTECTION_FLAGS>,
        ranges: impl Iterator<Item = &'a (*const c_void, *const c_void)>,
    ) -> [Option<*const c_void>; N] {
        Self::search_in_ranges(protection, ranges, |search_base, region_size, addresses: &mut [Option<*const c_void>]| {
            let search_region =
                unsafe { std::slice::from_raw_parts(search_base, region_size) };
            for (&pattern, address) in patterns
                .iter()
                .zip(addresses.iter_mut())
                .filter(|(_, a)| a.is_none())
            {
                if let Some(offset) = memmem::find(search_region, pattern) {
                    let found_address = unsafe { search_base.add(offset) } as *const c_void;
                    *address = Some(found_address);
                }
            }

            addresses.iter().all(Option::is_some)
        })
    }

    /// Check if the given addresses are found within the provided memory regions with the specified
    /// protection flags
    ///
    /// # Arguments
    ///
    /// * `addresses` - The addresses to search for
    /// * `protection` - If provided, only search memory regions matching one of the specified protection flags
    /// * `ranges` - An iterator of (start, end) address tuples defining the address ranges to search
    ///
    /// # Return
    ///
    /// An array of `bool` with the same number of elements as the `addresses` argument. Each element
    /// in the return will be true if the corresponding address was found or false if it wasn't.
    pub fn find_addresses_in_ranges<'a, const N: usize>(
        addresses: &[usize; N],
        protection: Option<PAGE_PROTECTION_FLAGS>,
        ranges: impl Iterator<Item = &'a (*const c_void, *const c_void)>,
    ) -> [bool; N] {
        Self::search_in_ranges(protection, ranges, |search_base, region_size, flags: &mut [bool]| {
            for (&address, flag) in addresses
                .iter()
                .zip(flags.iter_mut())
                .filter(|(_, f)| !**f)
            {
                let start = search_base as usize;
                let end = start + region_size;
                if address >= start && address < end {
                    *flag = true;
                }
            }

            flags.iter().all(|&f| f)
        })
    }

    /// Enumerate the modules loaded in the current process
    ///
    /// This method must be called once prior to attempting any searches that filter by module.
    pub fn discover_modules(&mut self) -> Result<()> {
        let mut modules = [HMODULE::default(); 1024];
        let mut bytes_needed = 0;
        let hproc = unsafe { GetCurrentProcess() };
        unsafe {
            EnumProcessModules(
                hproc,
                modules.as_mut_ptr(),
                size_of_val(&modules) as u32,
                &mut bytes_needed,
            )
        }?;

        let num_modules =
            std::cmp::min(bytes_needed as usize / size_of::<HMODULE>(), modules.len());
        for &module in &modules[..num_modules] {
            let mut name_utf16 = [0; MAX_PATH as usize];
            let module_name = unsafe {
                let num_chars = GetModuleBaseNameW(hproc, Some(module), &mut name_utf16) as usize;
                if num_chars == 0 || num_chars >= name_utf16.len() {
                    continue;
                }

                match PWSTR::from_raw(name_utf16.as_mut_ptr()).to_string() {
                    Ok(name) => name,
                    Err(_) => continue,
                }
            }
                .to_lowercase();

            let mut module_info = MODULEINFO::default();
            unsafe {
                GetModuleInformation(
                    hproc,
                    module,
                    &mut module_info,
                    size_of_val(&module_info) as u32,
                )?;
                let base = module_info.lpBaseOfDll as *const c_void;
                self.modules.insert(
                    module_name,
                    (base, base.add(module_info.SizeOfImage as usize)),
                );
            }
        }

        Ok(())
    }

    fn get_module_ranges<'b, 'a: 'b, 'c: 'b>(
        &'a self,
        modules: &'b [&'c str],
    ) -> impl Iterator<Item = &'a (*const c_void, *const c_void)> + 'b {
        modules
            .iter()
            .filter_map(|&module_name| self.modules.get(&module_name.to_lowercase()))
    }

    /// Search for byte strings in process memory
    ///
    /// # Arguments
    ///
    /// * `patterns` - The byte strings to search for
    /// * `protection` - If provided, only search memory regions matching one of the specified protection flags
    /// * `modules` - If not empty, only search memory regions belonging to the specified modules
    ///
    /// # Return
    ///
    /// An array of `Option<*const c_void>` with the same number of elements as the `patterns` argument.
    /// If the corresponding byte string was found, the value will be `Some(ptr)`, where `ptr` is a
    /// pointer to the location where the byte string was found. If the byte string was not found,
    /// the element in the return array will be `None`.
    pub fn find_bytes<const N: usize, const M: usize>(
        &self,
        patterns: &[&[u8]; N],
        protection: Option<PAGE_PROTECTION_FLAGS>,
        modules: &[&str; M],
    ) -> [Option<*const c_void>; N] {
        if M > 0 {
            Self::find_bytes_in_ranges(patterns, protection, self.get_module_ranges(modules))
        } else {
            // we'll use the standard page size as the minimum address
            Self::find_bytes_in_ranges(
                patterns,
                protection,
                [&(0x1000 as *const c_void, usize::MAX as *const c_void)].into_iter(),
            )
        }
    }

    /// Check if the given addresses are found within process memory with the specified protection flags
    ///
    /// # Arguments
    ///
    /// * `addresses` - The addresses to search for
    /// * `protection` - If provided, only search memory regions matching one of the specified protection flags
    /// * `modules` - If not empty, only search memory regions belonging to the specified modules
    ///
    /// # Return
    ///
    /// An array of `bool` with the same number of elements as the `addresses` argument. Each element
    /// in the return will be true if the corresponding address was found or false if it wasn't.
    pub fn find_addresses<const N: usize, const M: usize>(
        &self,
        addresses: &[usize; N],
        protection: Option<PAGE_PROTECTION_FLAGS>,
        modules: &[&str; M],
    ) -> [bool; N] {
        if M > 0 {
            Self::find_addresses_in_ranges(addresses, protection, self.get_module_ranges(modules))
        } else {
            // we'll use the standard page size as the minimum address
            Self::find_addresses_in_ranges(
                addresses,
                protection,
                [&(0x1000 as *const c_void, usize::MAX as *const c_void)].into_iter(),
            )
        }
    }

    /// Shorthand for calling `find_addresses` with a protection of `PAGE_READWRITE | PAGE_WRITECOPY`
    pub fn find_addresses_write<const N: usize, const M: usize>(
        &self,
        addresses: &[usize; N],
        modules: &[&str; M],
    ) -> [bool; N] {
        self.find_addresses(addresses, Some(PAGE_READWRITE | PAGE_WRITECOPY), modules)
    }

    /// Shorthand for calling `find_addresses` with a protection of `PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE`
    pub fn find_addresses_exec<const N: usize, const M: usize>(
        &self,
        addresses: &[usize; N],
        modules: &[&str; M],
    ) -> [bool; N] {
        self.find_addresses(addresses, Some(PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE), modules)
    }
}
