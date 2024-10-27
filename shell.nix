{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  buildInputs = [
    pkgs.rustup
  ];

  shellHook = ''
    DATA_DIR=/tmp/rust/lum
    export RUSTUP_HOME=$DATA_DIR/rustup
    export CARGO_HOME=$DATA_DIR/cargo
    export PATH=$CARGO_HOME/bin:$PATH
    mkdir -p $CARGO_HOME
    mkdir -p $RUSTUP_HOME

    rustup default stable
    rustup update
    cargo fetch

    echo
    echo
    echo

    echo "Rustup installed at $RUSTUP_HOME"
    echo "Cargo installed at $CARGO_HOME"
    echo $(cargo --version)
  '';
}
