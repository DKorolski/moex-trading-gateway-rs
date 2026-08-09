fn main() {
    let sequential = std::env::args()
        .skip(1)
        .any(|argument| argument == "--sequential");
    let artifact = if sequential {
        strategy_runtime_core::stage5g_h_sequential_lifecycle_artifact_json_pretty()
    } else {
        strategy_runtime_core::stage5g_g_lifecycle_artifact_json_pretty()
    };
    println!("{}", artifact);
}
