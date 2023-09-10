{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, flake-utils, nixpkgs, nixpkgs-mozilla, crane, advisory-db } @ inputs:
    flake-utils.lib.eachSystem [
      flake-utils.lib.system.aarch64-darwin
      flake-utils.lib.system.x86_64-linux
    ]
      (system:
        let
          pkgs = (import nixpkgs) {
            inherit system;

            overlays = [
              (import nixpkgs-mozilla)
            ];
          };

          toolchain = (pkgs.rustChannelOf {
            rustToolchain = ./rust-toolchain.toml;  # if git repo => local file has to be tracked in git
            sha256 = "sha256-Q9UgzzvxLi4x9aWUJTn+/5EXekC98ODRU1TwhUs9RnY=";
            #        ^ After you run `nix build`, replace this with the actual
            #          hash from the error message
          });

          craneLib = crane.lib.${system}.overrideToolchain toolchain.rust;

          # Some non-rust files are read during compilation so we have to make
          # sure they are included in the nix environment.
          # - `.bin` is a binary protoc file, used in dwh-exporter to communicate protobuf schema
          # - `.env` environment variables used in tests
          # - `.json` test data imported in tests
          # - `.sql` postgres migrations
          # - `.graphql` graphql schema definition files
          # - `.yml` spec definitions
          binFilter = path: _type: (!isNull (builtins.match ".*bin$" path));
          crtFilter = path: _type: (!isNull (builtins.match ".*crt$" path));
          envFilter = path: _type: (!isNull (builtins.match ".*env$" path));
          graphqlFilter = path: _type: (!isNull (builtins.match ".*graphql$" path));
          jsonFilter = path: _type: (!isNull (builtins.match ".*test_data.*json$" path));
          keyFilter = path: _type: (!isNull (builtins.match ".*key$" path));
          mmdbFilter = path: _type: (!isNull (builtins.match ".*mmdb$" path));
          pemFilter = path: _type: (!isNull (builtins.match ".*pem$" path));
          sqlFilter = path: _type: (!isNull (builtins.match ".*sql$" path));
          ymlFilter = path: _type: (!isNull (builtins.match ".*yaml$" path));
          filterSourceFiles = path: type:
            (binFilter path type) ||
            (crtFilter path type) ||
            (envFilter path type) ||
            (graphqlFilter path type) ||
            (jsonFilter path type) ||
            (keyFilter path type) ||
            (mmdbFilter path type) ||
            (pemFilter path type) ||
            (sqlFilter path type) ||
            (ymlFilter path type) ||
            (craneLib.filterCargoSources path type);

          # Common derivation arguments used for all builds
          commonArgs = {
            src = pkgs.lib.cleanSourceWith {
              src = pkgs.lib.cleanSource ./.;
              filter = filterSourceFiles;
            };

            buildInputs = with pkgs; [
              # Add extra build inputs here, etc.
            ];

            nativeBuildInputs = with pkgs;
              [
                pkgs.libiconv
                pkgs.zlib
                pkgs.cmake
                pkgs.protobuf
                pkgs.cacert
                

                # required for the diesel crate
                pkgs.postgresql

                # required for the openssl crate
                pkgs.pkg-config
                pkgs.openssl
                pkgs.perl
                pkgs.gnumake
              ]
              ++ nixpkgs.lib.optionals (system == "aarch64-darwin") [
                pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
                pkgs.darwin.apple_sdk.frameworks.Security
              ];
          };

          traceAndReturn = e: builtins.trace (builtins.tryEval e) e;

          matrix =
            (builtins.foldl'
              ({ packages, apps }: profile:
                let
                  profileArgs = commonArgs // {
                    CARGO_PROFILE = profile;
                    pname = "cobalt-${profile}";
                    doCheck = false;
                    doInstallCargoArtifacts = false;
                  };
                  # Build *just* the cargo dependencies, so we can reuse
                  # all of that work (e.g. via cachix) when running in CI
                  cargoArtifacts = craneLib.buildDepsOnly (profileArgs // {
                    doInstallCargoArtifacts = true;
                    # https://github.com/ipetkov/crane/issues/312
                    extraDummyScript = "rm -f $(find $out | grep bin/crane-dummy/main.rs)";
                  });
                  cargoAudit = craneLib.cargoAudit (profileArgs // {
                    inherit cargoArtifacts advisory-db;
                    cargoAuditExtraArgs = "--ignore RUSTSEC-2022-0090 --ignore RUSTSEC-2020-0071";
                  });
                  cargoClippy = craneLib.buildPackage (profileArgs // {
                    inherit cargoArtifacts;
                    buildPhaseCargoCommand = ''
                      cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)
                      cargoWithProfile clippy --message-format json -- -A clippy::all -W clippy::expect_used -W clippy::panic >"$cargoBuildLog"
                    '';
                    installPhaseCommand = ''mkdir -p $out && mv "$cargoBuildLog" $out/clippy-build-log.json'';
                  });
                  cargoClippyTest = craneLib.buildPackage (profileArgs // {
                    inherit cargoArtifacts;
                    buildPhaseCargoCommand = ''
                      cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)
                      cargoWithProfile clippy --message-format json --all-features --all-targets -- -W clippy::unwrap_used -W warnings >"$cargoBuildLog"
                    '';
                    installPhaseCommand = ''mkdir -p $out && mv "$cargoBuildLog" $out/clippy-build-log.json'';
                  });
                  cargoFmt = craneLib.cargoFmt (profileArgs // {
                    inherit cargoArtifacts;
                  });
                  cargoTarpaulin = craneLib.cargoTarpaulin (profileArgs // {
                    inherit cargoArtifacts;
                  });
                  cargoTestArchiveName = "nextest.tar.zst";
                  cargoTestBuild = craneLib.cargoNextest (profileArgs // {
                    inherit cargoArtifacts;
                    doCheck = false;
                    buildPhaseCargoCommand = "cargo nextest archive --archive-file ${cargoTestArchiveName}";
                    installPhaseCommand = "mkdir -p $out && mv ${cargoTestArchiveName} $out/";
                  });
                  cargoTestRunScript = ''
                  ${pkgs.cargo-nextest}/bin/cargo-nextest nextest run --archive-file ${cargoTestBuild}/${cargoTestArchiveName} --workspace-remap .
                  '';
                  cargoDeny = craneLib.mkCargoDerivation (profileArgs // {
                    inherit cargoArtifacts;
                    pnameSuffix = "-deny";
                    buildPhaseCargoCommand = "cargo-deny check bans licenses";
                    nativeBuildInputs = (profileArgs.nativeBuildInputs or [ ]) ++ [ pkgs.cargo-deny ];
                  });
                  prevPackages = packages;
                in
                (builtins.foldl'
                  ({ packages, apps }: crateName:
                    let
                      prevPackages = packages;
                      prevApps = apps;
                      packageName = "${crateName}-${profile}";
                      crate = craneLib.buildPackage (profileArgs // {
                        inherit cargoArtifacts;
                        cargoExtraArgs = "-p ${crateName}";
                        pname = "${crateName}-${profile}";
                      });
                    in
                    rec {
                      packages = prevPackages // {
                        "${packageName}" = crate;
                        "docker-${packageName}" = pkgs.dockerTools.buildImage {
                          name = crateName;
                          tag = "latest";
                          copyToRoot = pkgs.buildEnv {
                            name = "image-root";
                            paths = [ crate ];
                            pathsToLink = [ "/bin" ];
                          };
                          config = {
                            Entrypoint = [ "${crate}/bin/${crateName}" ];
                            Env = [ "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt" ];
                          };
                        };
                      };
                      apps = prevApps // {
                        "${packageName}" = {
                          type = "app";
                          program = "${packages.${packageName}}/bin/${crateName}";
                        };
                      };
                    }
                  )
                  {
                    packages = prevPackages // {
                      "cobalt-audit-${profile}" = cargoAudit;
                      "cobalt-clippy-${profile}" = cargoClippy;
                      "cobalt-clippy-test-${profile}" = cargoClippyTest;
                      "cobalt-deny-${profile}" = cargoDeny;
                      "cobalt-deps-${profile}" = cargoArtifacts;
                      "cobalt-fmt-${profile}" = cargoFmt;
                      "cobalt-coverage-${profile}" = cargoTarpaulin;
                      "cobalt-test-${profile}-build" = cargoTestBuild;
                      "cobalt-test-${profile}" = pkgs.writeShellScriptBin "cobalt-test-${profile}" cargoTestRunScript;
                    };
                    inherit apps;
                  } [
                  "crate-name"
                ]))
              {
                packages = { machete = pkgs.cargo-machete; };
                apps = { };
              } [
              "dev"
              "release"
            ]);

        in
        {
          packages = matrix.packages;
          apps = matrix.apps // { default = matrix.apps.sky-cli-release; };
          formatter = pkgs.nixpkgs-fmt;

          devShell = pkgs.mkShell {
            packages = [
              toolchain.rust
              toolchain.rust-src
              pkgs.cargo-machete
              pkgs.cargo-nextest
              pkgs.cargo-hakari
              pkgs.pre-commit
            ];
            nativeBuildInputs = commonArgs.nativeBuildInputs;
            shellHook = ''
              export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
              export RUST_SRC_PATH="${toolchain.rust-src}/lib/rustlib/src/rust/library";
            '';
          };
        });
}
