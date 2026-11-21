#!/bin/sh
# Output used by the CI headless render smoke test: colors, attributes and Unicode.
printf 'hivetty render smoke test\n'
printf '\033[31mred \033[32mgreen \033[1;34mbold-blue\033[0m \033[7m inverse \033[0m end\n'
printf '\033[4munderline\033[0m 256:\033[38;5;208morange\033[0m rgb:\033[38;2;120;200;255msky\033[0m\n'
printf 'Unicode: ñ 日本 ✓ → λ\n'
printf 'box: ┌──┬──┐ │ab│cd│ └──┴──┘\n'
