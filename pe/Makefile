build:
	cargo build --release

install:
	cp ./target/release/project-env $(HOME)/bin/releases/pe
	cd $(HOME)/bin && chmod 755 ./releases/pe && ln -s ./releases/pe pe
