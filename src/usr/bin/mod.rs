pub mod sh;

pub fn execute_binary(cwd_ino: usize, path_or_name: &str, _args: &[&str]) -> bool {
    match crate::kernel::loader::execute_file(cwd_ino, path_or_name) {
        Ok(status) => {
            crate::kernel::error::handle_process_termination(path_or_name, status, "Process exited cleanly. Omedetou, Shinji!");
            true
        }
        Err(err) => {
            crate::kernel::error::handle_process_termination(
                path_or_name,
                crate::kernel::error::ProcessStatus::Crashed,
                err,
            );
            false
        }
    }
}