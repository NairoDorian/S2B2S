# GENERATED FILE — DO NOT EDIT.
#
# Source: `scripts/app-meta.ts` — run `bun run meta:sync` to regenerate.
#
# Home-manager module for ZER0 speech-to-text.
#
# Provides a systemd user service for autostart.
# Usage: imports = [ zer0.homeManagerModules.default ];
#        services.zer0.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.zer0;
in
{
  options.services.zer0 = {
    enable = lib.mkEnableOption "ZER0 speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "zer0.packages.\${system}.zer0";
      description = "The ZER0 package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.zer0 = {
      Unit = {
        Description = "ZER0 speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/zer0";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
