pub(super) fn print_usage() {
    println!(
        "Usage: peekaboox [--daemon] [--socket <path>] <capture|see|agent|windows|window|elements|set-value|perform-action|ocr|compare|state|vision-elements|desktop|doctor|click|move|drag|swipe|type|paste|hotkey|press|scroll|app|launcher|workspace|dialog|menu|config|permissions|tools|completions|clean>"
    );
    println!("Try:   peekaboox capture --output screenshot.png");
    println!("Try:   peekaboox see --annotate --json");
    println!("Try:   peekaboox agent --goal \"Inspect the current desktop\" --dry-run");
    println!("Try:   peekaboox --daemon capture-delta --stream agent-loop");
    println!("Try:   peekaboox capture-backends");
    println!("Try:   peekaboox capture-dmabuf");
    println!("Try:   peekaboox plugins --path examples/plugins");
    println!(
        "Try:   peekaboox plugin-call org.peekaboox.examples.system-info system_info.uname --path examples/plugins --json"
    );
    println!("Try:   peekaboox --daemon windows");
    println!("Try:   peekaboox windows");
    println!("Try:   peekaboox elements --role \"push button\" --state enabled");
    println!("Try:   peekaboox ocr --region 10,20,400,120 --language eng");
    println!("Try:   peekaboox compare before.png after.png --max-changed-ratio 0.01");
    println!("Try:   peekaboox state frame1.png frame2.png frame3.png");
    println!("Try:   peekaboox vision-elements screenshot.png --min-width 8");
    println!("Try:   peekaboox desktop focus --app telegram");
    println!("Try:   peekaboox desktop click --app telegram --target search-input");
    println!("Try:   peekaboox doctor --json");
    println!("Try:   peekaboox click --x 100 --y 200");
    println!("Try:   peekaboox click --text \"Submit\"");
    println!("Try:   peekaboox move --x 100 --y 200");
    println!("Try:   peekaboox drag --from 100,200 --to 300,240 --duration-ms 350");
    println!("Try:   peekaboox scroll down --amount 3 --dry-run");
    println!("Try:   peekaboox type \"Hello World\"");
    println!("Try:   peekaboox hotkey ctrl+s");
    println!("Try:   peekaboox tools");
}

pub(super) fn print_capture_usage() {
    println!(
        "Usage: peekaboox capture [--output <path>|--stdout] [--format png|jpeg|xwd] [--quality 1..100] [--region x,y,width,height] [--window-id <id>|--app <name>|--window-title <text>|--title-regex <regex>] [--json] [--include-semantic-tree] [--no-overwrite]"
    );
}

pub(super) fn print_capture_delta_usage() {
    println!(
        "Usage: peekaboox --daemon capture-delta [--stream <id>] [--reset] [--region x,y,width,height | --window-id <id>] [--threshold <0-255>] [--low-bandwidth|--full-frame] [--json]"
    );
}

pub(super) fn print_capture_backends_usage() {
    println!(
        "Usage: peekaboox [--daemon] capture-backends [--output <path>|--format png|jpeg|xwd] [--region x,y,width,height] [--diagnose|--all] [--probe none|file|frame|region|dmabuf|all] [--json]"
    );
}

pub(super) fn print_capture_dmabuf_usage() {
    println!("Usage: peekaboox capture-dmabuf [--import <compute|egl|egl-texture>]");
}

pub(super) fn print_plugins_usage() {
    println!("Usage: peekaboox [--daemon] plugins [--path <plugin-dir-or-manifest>]... [--json]");
}

pub(super) fn print_plugin_call_usage() {
    println!(
        "Usage: peekaboox [--daemon] plugin-call <plugin-id> <tool> [--arguments-json <json>] [--path <plugin-dir-or-manifest>]... [--timeout-ms <ms>] [--max-output-bytes <n>] [--json]"
    );
}

pub(super) fn print_windows_usage() {
    println!(
        "Usage: peekaboox windows [--id <id>] [--app <app>] [--title <text>] [--title-regex <regex>] [--focused] [--limit <n>] [--sort backend|focused|title|app|area|id|state] [--backend auto|gnome|at-spi|xdotool] [--diagnose] [--json]"
    );
}

pub(super) fn print_elements_usage() {
    println!(
        "Usage: peekaboox elements [<selector>|--selector <query>] [--id <id>] [--role <role>] [--role-exact <role>] [--role-regex <regex>] [--text <label>] [--text-exact <label>] [--text-regex <regex>] [--state <state>] [--not-state <state>] [--bounds x,y,w,h] [--contains x,y] [--within x,y,w,h] [--intersects x,y,w,h] [--min-width <px>] [--min-height <px>] [--min-confidence <float>] [--app <app>] [--window-title <text>] [--window-id <id>] [--limit <n>] [--vision-fallback] [--vision-region x,y,w,h] [--vision-threshold <1-255>] [--vision-min-width <px>] [--vision-min-height <px>] [--vision-min-component-pixels <px>] [--vision-max-elements <n>] [--vision-merge-distance <px>] [--json]"
    );
}

pub(super) fn print_ocr_usage() {
    println!(
        "Usage: peekaboox ocr [--image <path> | --window-id <id> | --window-title <text> | --app <app>] [--region x,y,width,height] [--language <code>] [--psm <0-13>] [--oem <0-3>] [--dpi <n>] [--min-confidence <0..1>] [--whitelist <chars>] [--config key=value] [--scale <0.1..8>] [--grayscale] [--threshold <0-255>] [--invert] [--contrast <-255..255>] [--deskew] [--words] [--json]"
    );
}

pub(super) fn print_compare_usage() {
    println!(
        "Usage: peekaboox compare [--expected <path>] [--actual <path>] [--region x,y,width,height] [--ignore-region x,y,width,height]... [--threshold 0..255] [--max-changed-ratio 0.0..1.0] [--max-changed-pixels <n>] [--max-mae 0..255] [--max-channel-delta 0..255] [--size-policy error|common-region|resize-actual] [--alpha ignore|compare] [--diff-output <path>] [--report <path>] [--no-fail] [--json]"
    );
    println!("       peekaboox compare <expected-path> <actual-path>");
}

pub(super) fn print_ui_state_usage() {
    println!(
        "Usage: peekaboox state [--image <path>]... [--region x,y,width,height] [--ignore-region x,y,width,height]... [--threshold 0..255] [--stable-max-changed-ratio 0.0..1.0] [--stable-max-changed-pixels <n>] [--stable-max-mae 0..255] [--stable-max-channel-delta 0..255] [--loading-min-changed-ratio 0.0..1.0] [--loading-min-changed-pixels <n>] [--required-stable-transitions <n>] [--size-policy error|common-region|resize-actual] [--alpha ignore|compare] [--json]"
    );
    println!("       peekaboox state <image-path> <image-path> [more-image-paths...]");
}

pub(super) fn print_vision_elements_usage() {
    println!(
        "Usage: peekaboox vision-elements [--image <path>] [--region x,y,width,height] [--ignore-region x,y,width,height]... [--threshold 1..255] [--min-width <pixels>] [--max-width <pixels>] [--min-height <pixels>] [--max-height <pixels>] [--min-component-pixels <pixels>] [--min-confidence 0.0..1.0] [--min-area <pixels>] [--max-area <pixels>] [--max-elements <n>] [--merge-distance <pixels>] [--padding <pixels>] [--sort position|area|confidence] [--mask-output <path>] [--overlay-output <path>] [--json]"
    );
    println!("       peekaboox vision-elements <image-path>");
}

pub(super) fn print_desktop_usage() {
    println!(
        "Usage: peekaboox desktop profiles [--app <app>] [--target <target>] [--command <name>] [--desktop-id <id>] [--supports <capability>] [--check|--availability] [--installed|--available] [--json]"
    );
    println!(
        "       External desktop profiles: set PEEKABOOX_DESKTOP_PROFILE_PATH to JSON files or directories"
    );
    println!(
        "Usage: peekaboox desktop focus --app <app> [--window-id <id>|--window-title <text>] [--verify] [--json] [--no-overview] [--no-launch] [--wait-ms <ms>] [--overview-wait-ms <ms>]"
    );
    println!(
        "Usage: peekaboox desktop locate --app <app> --target <target> [--window-id <id>|--window-title <text>] [--image <path>] [--json] [--no-accessibility]"
    );
    println!(
        "Usage: peekaboox desktop click --app <app> --target <target> [--window-id <id>|--window-title <text>] [--button left|middle|right] [--image <path>] [--dry-run] [--verify] [--json] [--no-accessibility]"
    );
    println!(
        "Usage: peekaboox desktop drag --app <app> --target <target> --from-ratio <x,y> --to-ratio <x,y> [--window-id <id>|--window-title <text>] [--duration-ms <ms>] [--button left|middle|right] [--image <path>] [--dry-run] [--verify] [--json] [--no-accessibility]"
    );
    println!(
        "Usage: peekaboox desktop type-into --app <app> --target <target> [--window-id <id>|--window-title <text>] [--clear] [--image <path>] [--dry-run] [--verify] [--json] <text>"
    );
    println!(
        "Usage: peekaboox desktop assert --app <app> --target <target> [--window-id <id>|--window-title <text>] [--present|--active|--not-active|--contains <text>] [--image <path>] [--json]"
    );
    println!(
        "       peekaboox desktop assert-not --app <app> --target <target> [--window-title <text>] [--present|--active|--contains <text>]"
    );
}

pub(super) fn print_click_usage() {
    println!(
        "Usage: peekaboox click (--x <px> --y <px>|--to x,y|--text <label>|--selector <query>|--ratio x,y [--region x,y,w,h|--window-id <id>|--app <name>|--window-title <text>|--title-regex <regex>]) [--button left|middle|right] [--bounds allow|clamp|fail] [--backend auto|uinput|ydotool|xdotool] [--restore] [--dry-run] [--vision-fallback] [--json]"
    );
}

pub(super) fn print_move_usage() {
    println!(
        "Usage: peekaboox move [--x <px> --y <px>|--to x,y|--relative dx,dy|--dx <px> --dy <px>|--ratio x,y [--region x,y,w,h|--window-id <id>|--app <name>|--window-title <text>]|--current-position] [--duration-ms <ms>] [--steps <n>] [--bounds allow|clamp|fail|--clamp|--fail-out-of-bounds] [--backend auto|uinput|ydotool|xdotool] [--restore] [--dry-run] [--json]"
    );
}

pub(super) fn print_drag_usage() {
    println!(
        "Usage: peekaboox drag (--from x,y|--from-x <px> --from-y <px>|--from-current|--from-ratio x,y) (--to x,y|--to-x <px> --to-y <px>|--to-ratio x,y) [--region x,y,w,h|--window-id <id>|--app <name>|--window-title <text>|--title-regex <regex>] [--button left|middle|right] [--duration-ms <ms>] [--steps <n>] [--bounds allow|clamp|fail] [--backend auto|uinput|xdotool] [--restore] [--dry-run] [--json]"
    );
}

pub(super) fn print_type_usage() {
    println!(
        "Usage: peekaboox type [--dry-run] [--json] [--backend auto|wtype|ydotool|xdotool] [--typing-speed <cps>|--key-delay-ms <ms>] [--delay-ms <ms>] [--paste] [--preserve-clipboard] [--clipboard-backend auto|wl-copy|xclip|xsel] [--hotkey-backend auto|ydotool|xdotool] [--restore-delay-ms <ms>] [--restore-policy strict|best-effort|off] [--text <text>|--stdin|--file <path>|-- <text>|<text>...]"
    );
}

pub(super) fn print_paste_usage() {
    println!(
        "Usage: peekaboox paste [--dry-run] [--json] [--preserve-clipboard] [--clipboard-backend auto|wl-copy|xclip|xsel] [--hotkey-backend auto|ydotool|xdotool] [--delay-ms <ms>] [--restore-delay-ms <ms>] [--restore-policy strict|best-effort|off] [--text <text>|--stdin|--file <path>|-- <text>|<text>...]"
    );
}

pub(super) fn print_hotkey_usage() {
    println!(
        "Usage: peekaboox hotkey [--dry-run] [--json] [--backend auto|ydotool|xdotool] [--delay-ms <ms>] [--key-delay-ms <ms>] [--repeat <n>] [--interval-ms <ms>] [--release-before] [--release-after] [--key <key>] [-- <key-or-combo>...] <key-or-combo> [more-keys]"
    );
}

pub(super) fn print_see_usage() {
    println!(
        "Usage: peekaboox see [capture options] [--id <snapshot-id>] [--output-dir <dir>] [--annotate] [--no-elements] [--json]"
    );
}

pub(super) fn print_agent_usage() {
    println!(
        "Usage: peekaboox agent [run] --goal <goal> [--dry-run] [--resume <session-id>] [--model <name>] [--max-steps <n>] [--json]"
    );
    println!("       peekaboox agent list-sessions [--json]");
}

pub(super) fn print_set_value_usage() {
    println!(
        "Usage: peekaboox set-value --selector <query>|--id <element-id> --value <text-or-number> [--dry-run] [--json]"
    );
}

pub(super) fn print_perform_action_usage() {
    println!(
        "Usage: peekaboox perform-action --selector <query>|--id <element-id> [--action <name>|--index <n>] [--dry-run] [--json]"
    );
}

pub(super) fn print_scroll_usage() {
    println!(
        "Usage: peekaboox scroll [up|down|left|right|--direction <dir>] [--amount <n>] [--at x,y] [--delay-ms <ms>] [--bounds allow|clamp|fail] [--backend auto|ydotool|xdotool] [--dry-run] [--json]"
    );
}

pub(super) fn print_app_usage() {
    println!("Usage: peekaboox app list [--json]");
    println!("       peekaboox app launch <desktop-id|command|uri> [--dry-run] [--json]");
    println!("       peekaboox app focus <app>");
    println!("       peekaboox app quit <process-name>");
}

pub(super) fn print_launcher_usage() {
    println!("Usage: peekaboox launcher list [--json]");
    println!("       peekaboox launcher open <desktop-id|command|uri>");
}

pub(super) fn print_window_usage() {
    println!("Usage: peekaboox window list [windows filters]");
    println!(
        "       peekaboox window focus|close|minimize|maximize|unmaximize|fullscreen [--id <id>|--app <app>|--title <text>] [--dry-run] [--json]"
    );
    println!("       peekaboox window move --id <id> --x <px> --y <px>");
    println!("       peekaboox window resize --id <id> --width <px> --height <px>");
}

pub(super) fn print_dialog_usage() {
    println!("Usage: peekaboox dialog list [elements filters]");
    println!("       peekaboox dialog click <label>");
    println!("       peekaboox dialog accept|cancel");
    println!("       peekaboox dialog type <selector> <text>");
}

pub(super) fn print_menu_usage() {
    println!("Usage: peekaboox menu list [elements filters]");
    println!("       peekaboox menu click <label>");
}

pub(super) fn print_workspace_usage() {
    println!("Usage: peekaboox workspace list [--json]");
    println!("       peekaboox workspace switch <index>");
    println!("       peekaboox workspace move-window <window-id> <index>");
}

pub(super) fn print_config_usage() {
    println!("Usage: peekaboox config show|init|path");
    println!("       peekaboox config get <key>");
    println!("       peekaboox config set <key> <json-or-string>");
}

pub(super) fn print_completions_usage() {
    println!("Usage: peekaboox completions <bash|zsh|fish>");
}

pub(super) fn print_clean_usage() {
    println!("Usage: peekaboox clean [--all] [--dry-run] [--json]");
}
