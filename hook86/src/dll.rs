pub use hook86_dll_main::dll_main;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CallReason {
    ProcessAttach { is_static_load: bool },
    ProcessDetach { is_process_exiting: bool },
    ThreadAttach,
    ThreadDetach,
}