CARGO_CODSPEED_VERSION := 6.0.0
SDK_BENCHMARKS := benchmarks/fixtures/sdk-benchmarks

$(SDK_BENCHMARKS)/environments:
	git submodule update --init -- $(SDK_BENCHMARKS)

.PHONY: benchmark benchmark-build benchmark-run
benchmark: benchmark-build benchmark-run

# Kept apart from benchmark-run: CodSpeed's simulation mode runs its whole command under Valgrind
benchmark-build: | $(SDK_BENCHMARKS)/environments
	cargo install --locked cargo-codspeed@$(CARGO_CODSPEED_VERSION)
	cd benchmarks && cargo codspeed build --locked

benchmark-run: | $(SDK_BENCHMARKS)/environments
	cd benchmarks && SDK_BENCHMARKS_ENVIRONMENTS=$(CURDIR)/$(SDK_BENCHMARKS)/environments cargo codspeed run
