# Reading the settings without Python: the value of a top-level key of a JSON file, in awk, for any
# formatting. A string comes out without its quotes, a number or true/false as written, an array one
# element per line. Exit 1: no such key or no such file. Sourced by install.sh; defines only.
# shellcheck shell=bash

json_field() { # file key
  [[ -r $1 ]] || return 1
  LC_ALL=C awk -v want="$2" '
    function ws(c) { return c == " " || c == "\t" || c == "\r" || c == "\n" }
    function skip_ws() { while (i <= n && ws(substr(s, i, 1))) i++ }
    function str(   c, e, out) {
      out = ""; i++
      while (i <= n) {
        c = substr(s, i, 1)
        if (c == "\\") {
          e = substr(s, i + 1, 1)
          if (e == "n") out = out "\n"
          else if (e == "t") out = out "\t"
          else if (e == "u") { out = out "?"; i += 4 }
          else out = out e
          i += 2
          continue
        }
        if (c == "\"") { i++; return out }
        out = out c; i++
      }
      return out
    }
    function tok(   b) {
      b = i
      while (i <= n) {
        c = substr(s, i, 1)
        if (c == "," || c == "}" || c == "]" || ws(c)) break
        i++
      }
      return substr(s, b, i - b)
    }
    function skip(   c, d) {
      c = substr(s, i, 1)
      if (c == "\"") { str(); return }
      if (c != "{" && c != "[") { tok(); return }
      d = 0
      while (i <= n) {
        c = substr(s, i, 1)
        if (c == "\"") { str(); continue }
        if (c == "{" || c == "[") d++
        else if (c == "}" || c == "]") { d--; if (d == 0) { i++; return } }
        i++
      }
    }
    function show(   c) {
      c = substr(s, i, 1)
      if (c == "\"") { print str(); return }
      if (c == "{") { print "{}"; return }
      if (c != "[") { print tok(); return }
      i++
      while (i <= n) {
        skip_ws(); c = substr(s, i, 1)
        if (c == "]") return
        if (c == ",") { i++; continue }
        if (c == "\"") print str()
        else if (c == "{" || c == "[") skip()
        else print tok()
      }
    }
    BEGIN { RS = "\001"; rc = 1 }
    {
      s = $0; n = length(s); i = 1
      skip_ws()
      if (substr(s, i, 1) != "{") exit
      i++
      while (i <= n) {
        skip_ws()
        c = substr(s, i, 1)
        if (c == ",") { i++; continue }
        if (c != "\"") exit
        k = str()
        skip_ws(); i++; skip_ws()
        if (k == want) { show(); rc = 0; exit }
        skip()
      }
    }
    END { exit rc }
  ' "$1"
}
# true when the value is true, a non-zero number or a non-empty string
json_true() { local v; v="$(json_field "$1" "$2")" && [[ -n $v && $v != false && $v != 0 && $v != null ]]; }
