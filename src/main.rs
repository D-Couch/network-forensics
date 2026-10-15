pub mod analysis;
pub mod dns;
pub mod event;
pub mod incidents;
pub mod interfaces;
pub mod journal;
pub mod monitor;
pub mod monitor_state;
pub mod probes;
pub mod replay;
pub mod report;
pub mod report_output;
pub mod storage;

mod cli;

pub(crate) use cli::next_event_id;

fn main() {
    cli::run();
}
