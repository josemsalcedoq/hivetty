# hivetty shell integration for fish: prompt marks (OSC 133) and the current directory (OSC 7).
# hivetty adds this directory to XDG_DATA_DIRS; fish loads vendor_conf.d files at startup.

status is-interactive; or exit
set -q __hivetty_loaded; and exit
set -g __hivetty_loaded 1

function __hivetty_osc7
    set -l encoded (string escape --style=url -- $PWD | string replace -a '%2F' '/')
    printf '\e]7;file://%s%s\e\\' (prompt_hostname) $encoded
end

function __hivetty_prompt --on-event fish_prompt
    __hivetty_osc7
    printf '\e]133;A\e\\'
end

function __hivetty_preexec --on-event fish_preexec
    printf '\e]133;C\e\\'
end

function __hivetty_postexec --on-event fish_postexec
    printf '\e]133;D;%d\e\\' $status
end
