build:
	cargo build --release

install:
	cp ./target/release/project-env $(HOME)/bin/releases/pe
	chmod 755 $(HOME)/bin/releases/pe && rm -f $(HOME)/bin/pe && ln -s $(HOME)/bin/releases/pe $(HOME)/bin/pe

clean:
	rm -rf ./target
