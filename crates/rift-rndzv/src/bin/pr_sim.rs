use std::fs;
use std::path::PathBuf;

use rift_rndzv::engine::sim::{
    render_csv, run_simulation, NatBehavior, NatConfig, NatSim, SimConfig, SimPeer,
};
use rift_rndzv::{Role, TimeModel};

fn main() {
    let output_dir = PathBuf::from("docs/simulations");
    let _ = fs::create_dir_all(&output_dir);

    let time_model = TimeModel {
        t0: 1_700_000_000,
        window_secs: 10,
        slot_ms: 250,
    };

    let cfg = SimConfig {
        time_model: time_model.clone(),
        duration_ms: 5_000,
        slot_step_ms: 250,
    };

    let mut peer_a = SimPeer {
        seed: [1u8; 32],
        role: Role::Caller,
        base_internal_port: 40000,
        public_ip: 0x0A000001,
        nat: NatSim::new(
            NatConfig {
                behavior: NatBehavior::PortPreserving,
                base_port: 50000,
                mapping_ttl_ms: 2_000,
            },
            42,
        ),
    };

    let mut peer_b = SimPeer {
        seed: [1u8; 32],
        role: Role::Callee,
        base_internal_port: 41000,
        public_ip: 0x0A000002,
        nat: NatSim::new(
            NatConfig {
                behavior: NatBehavior::SymmetricHashing,
                base_port: 51000,
                mapping_ttl_ms: 2_000,
            },
            1337,
        ),
    };

    let report = run_simulation(cfg, &mut peer_a, &mut peer_b);
    let csv = render_csv(&report);

    let out_path = output_dir.join("pr_sim_example.csv");
    let _ = fs::write(&out_path, csv);

    println!("simulation complete: success={} slots_attempted={}", report.success, report.slots_attempted);
    if let Some(slot) = report.first_success_slot {
        println!("first_success_slot={slot}");
    }
    println!("csv_written={}", out_path.display());
}
