determinism-check:
	cargo run -p design_cli --bin verify_cli -- determinism --input " @apps/cli/tests/integration/repl_file_target_routing.rs src/sample.rs を安全に改善して preview" --runs 5 --strict
	cargo run -p design_cli --bin verify_cli -- determinism --input "src/sample.rs を解析して修正して" --runs 5 --strict
	cargo run -p design_cli --bin verify_cli -- determinism --input " @apps/cli/tests/integration/repl_file_target_routing.rs src/sample.rs にログを追加して preview" --runs 5 --strict
