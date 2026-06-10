# Home-manager module for s2b2s speech-to-text
#
# Provides a systemd user service for autostart.
# Usage: imports = [ s2b2s.homeManagerModules.default ];
#        services.s2b2s.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.s2b2s;
in
{
  options.services.s2b2s = {
    enable = lib.mkEnableOption "s2b2s speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "s2b2s.packages.\${system}.s2b2s";
      description = "The s2b2s package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.s2b2s = {
      Unit = {
        Description = "s2b2s speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/s2b2s";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
