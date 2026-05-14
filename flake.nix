{
  description = "PeekabooX Linux-native AI desktop automation platform";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "peekaboox";
            version = "1.0.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkgs.pkg-config
            ];

            buildInputs = [
              pkgs.dbus
            ];

            cargoBuildFlags = [
              "-p"
              "peekaboox-cli"
              "-p"
              "peekabooxd"
            ];

            cargoTestFlags = [
              "--workspace"
            ];

            postInstall = ''
              install -Dm644 integrations/systemd/peekabooxd.service \
                $out/share/systemd/user/peekabooxd.service
              install -Dm644 integrations/systemd/peekabooxd-hardened.service \
                $out/share/systemd/user/peekabooxd-hardened.service
              mkdir -p $out/share/peekaboox
              cp -R examples $out/share/peekaboox/examples
              mkdir -p $out/share/doc/peekaboox
              cp README.md CHANGELOG.md $out/share/doc/peekaboox/
              cp -R docs/* $out/share/doc/peekaboox/
            '';
          };
        });

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.clippy
              pkgs.dbus
              pkgs.dpkg
              pkgs.pkg-config
              pkgs.python312
              pkgs.python312Packages.grpcio
              pkgs.python312Packages.protobuf
              pkgs.python312Packages.setuptools
              pkgs.python312Packages.wheel
              pkgs.rustc
              pkgs.rustfmt
              pkgs.tesseract
            ];
          };
        });
    };
}
