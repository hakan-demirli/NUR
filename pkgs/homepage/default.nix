{
  pkgs,
  lib,
  configFile ? null,
  ...
}:
let
  defaultConfig = {
    services = [ ];
    addresses = [ ];
    lan = [ ];
  };

  config = defaultConfig // (if configFile != null then lib.importJSON configFile else { });

  mkShortcut = { name, url }: ''{ name: "${name}", url: "${url}" }'';

  dataScript = ''
    <script>
      window.DEFAULT_SERVICES = [
        ${lib.concatStringsSep ",\n      " (map mkShortcut config.services)}
      ];
      window.DEFAULT_ADDRESSES = [
        ${lib.concatStringsSep ",\n      " (map mkShortcut config.addresses)}
      ];
      window.DEFAULT_LAN = [
        ${lib.concatStringsSep ",\n      " (map mkShortcut config.lan)}
      ];
    </script>
  '';
in
pkgs.runCommand "homepage" { } ''
    mkdir -p $out/share/homepage $out/bin

    cp ${./static/style.css} $out/share/homepage/style.css
    cp ${./static/script.js} $out/share/homepage/script.js

    substitute ${./static/index.html} $out/share/homepage/index.html \
      --replace "<!-- DATA_INJECTION_POINT -->" '${dataScript}'

    cat > $out/bin/homepage <<EOF
  #!${pkgs.bash}/bin/bash
  PORT=\''${1:-8100}
  cd $out/share/homepage
  echo "Serving homepage at http://localhost:\$PORT"
  ${pkgs.python3}/bin/python3 -m http.server "\$PORT"
  EOF
    chmod +x $out/bin/homepage
''
