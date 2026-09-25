# pe

Project environment manager for [direnv](https://direnv.net/).

Creates `bin/` and `.envrc` in the project root, puts project scripts on `PATH`,
and helps edit/store that setup.

Requires `direnv` and `$EDITOR` (falls back to `vim`).

## Build / install

```bash
make build
make install   # → ~/bin/pe
```

## Usage

```text
 $ pe                  # print .envrc
 $ pe init             # create bin/, .envrc; direnv allow
 $ pe reinit           # rewrite .envrc from scratch, then init
 $ pe e | edit         # edit .envrc
 $ pe ls [ls-opts]     # list scripts in $PROJECT_BIN
 $ pe fp | footprint   # save .envrc → config/footprint/$USER@$HOST.envrc
 $ pe <scriptname>     # create/edit executable script in bin/
 $ pe h | help         # this summary
```

`PROJECT_PATH` defaults to the current directory; `PROJECT_BIN` defaults to
`$PROJECT_PATH/bin`.

### What `init` writes into `.envrc`

- prepends `$PROJECT_PATH/bin` and `$PROJECT_PATH/node_modules/.bin` to `PATH`
- sets `PROJECT_PATH`, `PROJECT_BIN`, `PROJECT_NAME`, `PROJECT_CREATED`, `VAR_ROOT`

Set `DEBUG=true` to print the generated `.envrc` after init.
