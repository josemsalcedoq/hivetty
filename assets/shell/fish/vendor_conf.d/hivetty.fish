# hivetty shell integration for fish: prompt marks (OSC 133) and the current directory (OSC 7).
# hivetty adds this directory to XDG_DATA_DIRS; fish loads vendor_conf.d files at startup.

# Give child processes the user's own XDG_DATA_DIRS back.
if set -q HIVETTY_XDG_DATA_DIRS
    if test -n "$HIVETTY_XDG_DATA_DIRS"
        set -gx XDG_DATA_DIRS $HIVETTY_XDG_DATA_DIRS
    else
        set -e XDG_DATA_DIRS
    end
    set -e HIVETTY_XDG_DATA_DIRS
end

status is-interactive; or exit
set -q __hivetty_loaded; and exit
set -g __hivetty_loaded 1

function __hivetty_osc7
    set -l encoded (string escape --style=url -- $PWD | string replace -a '%2F' '/')
    printf '\e]7;file://%s%s\e\\' (prompt_hostname) $encoded
end

function __hivetty_prompt --on-event fish_prompt
    __hivetty_osc7
    # A blank line above the prompt where hivetty paints the project chips (docs/adr/0004).
    set -q HIVETTY_CHIPS; and echo
    printf '\e]133;A\e\\'
end

function __hivetty_preexec --on-event fish_preexec
    printf '\e]133;C\e\\'
end

function __hivetty_postexec --on-event fish_postexec
    printf '\e]133;D;%d\e\\' $status
end
