# hivetty shell integration for zsh: prompt marks (OSC 133) and the current directory (OSC 7).
# Loaded once per interactive shell. Safe to source by hand from .zshrc as well.

[[ -o interactive ]] || return 0
[[ -n "$__hivetty_loaded" ]] && return 0
typeset -g __hivetty_loaded=1
typeset -gi __hivetty_running=0

__hivetty_osc7() {
  # Percent-encode everything but unreserved characters and '/'.
  local LC_ALL=C char encoded=""
  local i
  for (( i = 1; i <= ${#PWD}; i++ )); do
    char="${PWD[i]}"
    case "$char" in
      [a-zA-Z0-9/._~-]) encoded+="$char" ;;
      *) encoded+=$(printf '%%%02X' "'$char") ;;
    esac
  done
  printf '\e]7;file://%s%s\e\\' "${HOST}" "$encoded"
}

__hivetty_precmd() {
  local ret=$?
  if (( __hivetty_running )); then
    printf '\e]133;D;%d\e\\' "$ret"
  fi
  __hivetty_running=0
  __hivetty_osc7
  # A blank line above the prompt where hivetty paints the project chips (docs/adr/0004).
  [[ -n "$HIVETTY_CHIPS" ]] && print
  printf '\e]133;A\e\\'
  # Mark the end of the prompt (start of input). Re-added every time because prompt themes
  # rebuild PS1.
  if [[ "$PS1" != *$'\e]133;B'* ]]; then
    PS1="$PS1%{"$'\e]133;B\e\\'"%}"
  fi
}

__hivetty_preexec() {
  printf '\e]133;C\e\\'
  __hivetty_running=1
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd __hivetty_precmd
add-zsh-hook preexec __hivetty_preexec
