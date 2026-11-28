//! erebor-agent — the background service that mines on this computer and, once set up,
//! lets paired controllers (Erebor Desktop, `erebor devices`) watch and control it.
//!
//!   erebor-agent [start]                 run in the foreground (what services run)
//!   erebor-agent setup                   prepare remote pairing and show a one-time code
//!   erebor-agent init [--listen ADDR] [--no-discovery]
//!                                        turn remote control on (TLS, port 7420)
//!   erebor-agent pair                    show a one-time pairing code (10 minutes)
//!   erebor-agent status                  remote setup, paired controllers, running?
//!   erebor-agent controllers             paired controllers
//!   erebor-agent revoke NAME|FINGERPRINT remove a controller's access
//!   erebor-agent rotate-key              new certificate (every controller pairs again)
//!   erebor-agent disable                 stop accepting remote controllers
//!   erebor-agent asic add IP [--port N] [--name NAME] | asic remove IP | asic list
//!                                        ASICs this agent watches as a gateway
//!
//! Options: --data-dir DIR (or EREBOR_DATA_DIR), --json, --version.
//! Changes to the remote setup apply when the agent (re)starts; pairing codes and
//! revocations apply at once.

fn main() {
    erebor_agent::cli::cli(std::env::args().skip(1));
}
