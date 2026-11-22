# hivetty shell integration for zsh: bootstrap.
#
# hivetty starts zsh with ZDOTDIR pointing at this directory. Here we put the user's ZDOTDIR
# back (zsh reads the rest of the startup files from it), load their .zshenv, then load the
# integration for interactive shells.

if [[ -n "${HIVETTY_ZDOTDIR+set}" ]]; then
  ZDOTDIR="$HIVETTY_ZDOTDIR"
  unset HIVETTY_ZDOTDIR
else
  unset ZDOTDIR
fi

if [[ -f "${ZDOTDIR:-$HOME}/.zshenv" ]]; then
  source "${ZDOTDIR:-$HOME}/.zshenv"
fi

if [[ -o interactive && -n "$HIVETTY_SHELL_DIR" && -f "$HIVETTY_SHELL_DIR/hivetty.zsh" ]]; then
  source "$HIVETTY_SHELL_DIR/hivetty.zsh"
fi
