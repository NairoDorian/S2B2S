# GENERATED FILE — DO NOT EDIT.
#
# Source: `scripts/app-meta.ts` — run `bun run meta:sync` to regenerate.
#
# NixOS module for ZER0 speech-to-text.
#
# Handles system-level configuration that the package wrapper cannot:
#   - udev rule for /dev/uinput (the handy-keys evdev grab re-injects
#     through virtual input devices; dotool / ydotool typing uses it too)
#
# Note: users must add themselves to the "input" group for evdev hotkey access.
#
# Usage in your flake:
#
#   inputs.zer0.url = "https://github.com/NairoDorian/S2B2S";
#
#   nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
#     modules = [
#       zer0.nixosModules.default
#       { programs.zer0.enable = true; }
#     ];
#   };
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.zer0;
in
{
  options.programs.zer0 = {
    enable = lib.mkEnableOption "ZER0 offline speech-to-text";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "zer0.packages.\${system}.zer0";
      description = "The ZER0 package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    # handy-keys' evdev grab creates virtual input devices via /dev/uinput.
    # Default permissions are crw------- root root — open it to the input group.
    services.udev.extraRules = ''
      KERNEL=="uinput", GROUP="input", MODE="0660"
    '';
  };
}
