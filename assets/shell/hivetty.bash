# hivetty shell integration for bash: prompt marks (OSC 133) and the current directory (OSC 7).
#
# hivetty starts bash with `--rcfile` pointing here, so bash does not read the user's files by
# itself: load them first, exactly as bash would have.

if [[ -n "$HIVETTY_BASH_LOGIN" ]]; then
  unset HIVETTY_BASH_LOGIN
  [[ -f /etc/profile ]] && . /etc/profile
  for __hivetty_f in ~/.bash_profile ~/.bash_login ~/.profile; do
    if [[ -f "$__hivetty_f" ]]; then
      . "$__hivetty_f"
      break
    fi
  done
  unset __hivetty_f
else
  [[ -f /etc/bash.bashrc ]] && . /etc/bash.bashrc
  [[ -f ~/.bashrc ]] && . ~/.bashrc
fi

[[ $- == *i* ]] || return 0
[[ -n "$__hivetty_loaded" ]] && return 0
__hivetty_loaded=1
__hivetty_first_prompt=1

__hivetty_osc7() {
  local LC_ALL=C char encoded="" i
  for (( i = 0; i < ${#PWD}; i++ )); do
    char="${PWD:i:1}"
    case "$char" in
      [a-zA-Z0-9/._~-]) encoded+="$char" ;;
      *) printf -v char '%%%02X' "'$char"; encoded+="$char" ;;
    esac
  done
  printf '\e]7;file://%s%s\e\\' "$HOSTNAME" "$encoded"
}

__hivetty_prompt_command() {
  local ret=$?
  # The previous command finished (bash runs this even for an empty line; that reports the
  # last real exit code again, which is harmless).
  if [[ -z "$__hivetty_first_prompt" ]]; then
    printf '\e]133;D;%d\e\\' "$ret"
  fi
  __hivetty_first_prompt=
  __hivetty_osc7
  # A blank line above the prompt where hivetty paints the project chips (docs/adr/0004).
  [[ -n "$HIVETTY_CHIPS" ]] && printf '\n'
  printf '\e]133;A\e\\'
  if [[ "$PS1" != *'133;B'* ]]; then
    PS1="$PS1"'\[\e]133;B\e\\\]'
  fi
  return $ret
}

# PS0 (bash 4.4+) is printed right before a command runs: the start of its output.
if (( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4) )); then
  PS0="${PS0}"'\[\e]133;C\e\\\]'
fi

if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
  PROMPT_COMMAND=(__hivetty_prompt_command "${PROMPT_COMMAND[@]}")
else
  PROMPT_COMMAND="__hivetty_prompt_command${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
fi
