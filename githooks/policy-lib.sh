#!/usr/bin/env bash
# Shared definitions for the git-side AI policy checks.
#
# Sourced by: githooks/commit-msg, githooks/pre-push,
#             hooks/human-authored-text/hook.sh
#
# TODO(jb-doc): why these live in one file instead of one copy per hook

# TODO(jb-comment): why scope is by path, and why the git hooks must no-op
# outside the hobby tree even though they install globally
policy_in_scope() {
    local dir="${1:-$PWD}"
    dir=$(cd "$dir" 2>/dev/null && pwd -P) || return 1
    case "$dir" in
        "$HOME"/fun_repos/*|"$HOME"/fun_repos) return 0 ;;
        "$HOME"/fun_worktrees/*)               return 0 ;;
        *)                                     return 1 ;;
    esac
}

# Matched case-insensitively with grep -E.
# TODO(jb-comment): why each of these five patterns is banned, and which one
# Bevy CI actually tests for
POLICY_ATTRIBUTION_RE='(co-authored-by:[[:space:]]*.*(claude|anthropic))|(claude-session:)|(generated with .*claude code)|(claude\.ai/code)|(noreply@anthropic\.com)|(🤖)'

# Printed on rejection, in the order the regex tests.
POLICY_ATTRIBUTION_DESC='  Co-authored-by: … Claude/Anthropic
  Claude-Session: …
  Generated with … Claude Code
  claude.ai/code links
  noreply@anthropic.com
  🤖'

policy_find_attribution() {
    grep -n -i -E "$POLICY_ATTRIBUTION_RE" "$@" 2>/dev/null
}

# TODO(jb-comment): why the placeholder token is spelled in exactly one place
# TODO(jb-comment): why the trailing colon is part of the pattern
POLICY_PLACEHOLDER_RE='TODO\(jb-(comment|doc)\):'

# TODO(jb-comment): why the expected email is checked at all, given `just
# git-identity` sets it — i.e. which repos it does not cover
POLICY_GIT_EMAIL='julian.bieber.1992@gmail.com'

policy_die() {
    printf '\n\033[31m✗ %s\033[0m\n\n' "$1" >&2
    shift
    [ "$#" -gt 0 ] && printf '%s\n' "$@" >&2
    printf '\n' >&2
    exit 1
}

# --- Command tokenizer -------------------------------------------------------
# TODO(jb-comment): why the gate classifies from argv rather than matching the
# raw command string, and what each of the two failure directions looked like
#
# policy_tokenize CMD
#   POLICY_SEGMENTS[]  one simple command per element; words joined by \x1f,
#                      each word prefixed `l` (literal) or `d` (expanded)
#   return 1           a construct that cannot be resolved
#
# Consumed and dropped: redirections and their targets, heredoc bodies,
# comments, line continuations. Recorded as `d`: $VAR, $(…), ${…}, `…`.

_policy_flush_word() {
    if [ "$_pt_hdpend" = 1 ]; then
        if [ "$_pt_have" = 1 ]; then
            if [ "$_pt_hdstrip" = 1 ]; then
                _pt_hd+=($'\x01'"$_pt_word")
            else
                _pt_hd+=("$_pt_word")
            fi
            _pt_hdpend=0
        fi
        _pt_word=""; _pt_have=0; _pt_dyn=0
        return 0
    fi
    if [ "$_pt_discard" = 1 ]; then
        if [ "$_pt_have" = 1 ]; then _pt_discard=0; fi
        _pt_word=""; _pt_have=0; _pt_dyn=0
        return 0
    fi
    if [ "$_pt_have" = 0 ]; then
        _pt_word=""; _pt_dyn=0
        return 0
    fi
    # TODO(jb-comment): why a group brace is dropped instead of kept as argv[0]
    if [ "$_pt_curn" = 0 ] && [ "$_pt_dyn" = 0 ]; then
        case $_pt_word in
            '{'|'}') _pt_word=""; _pt_have=0; return 0 ;;
        esac
    fi
    if [ "$_pt_dyn" = 1 ]; then
        _pt_cur+="d$_pt_word"$'\x1f'
    else
        _pt_cur+="l$_pt_word"$'\x1f'
    fi
    _pt_curn=$((_pt_curn + 1))
    _pt_word=""; _pt_have=0; _pt_dyn=0
}

_policy_flush_segment() {
    _policy_flush_word
    if [ "$_pt_curn" -gt 0 ]; then
        POLICY_SEGMENTS+=("$_pt_cur")
    fi
    _pt_cur=""; _pt_curn=0
}

_policy_scan_squote() {
    local rest=${_pt_s:_pt_i+1} body
    case $rest in
        *"'"*) ;;
        *) return 1 ;;
    esac
    body=${rest%%"'"*}
    _pt_word+=$body; _pt_have=1
    _pt_i=$((_pt_i + ${#body} + 2))
}

_policy_scan_dquote() {
    local j=$((_pt_i + 1)) ch
    while [ $j -lt $_pt_n ]; do
        ch=${_pt_s:j:1}
        case $ch in
            '\')     _pt_word+=${_pt_s:j+1:1}; _pt_have=1; j=$((j + 2)) ;;
            '"')     _pt_have=1; _pt_i=$((j + 1)); return 0 ;;
            '$'|'`') _pt_dyn=1; _pt_word+=$ch; _pt_have=1; j=$((j + 1)) ;;
            *)       _pt_word+=$ch; _pt_have=1; j=$((j + 1)) ;;
        esac
    done
    return 1
}

_policy_scan_expansion() {
    local j ch depth
    _pt_dyn=1; _pt_have=1
    if [ "${_pt_s:_pt_i:1}" = '`' ]; then
        j=$((_pt_i + 1))
        while [ $j -lt $_pt_n ]; do
            ch=${_pt_s:j:1}
            if [ "$ch" = '\' ]; then j=$((j + 2)); continue; fi
            if [ "$ch" = '`' ]; then _pt_i=$((j + 1)); return 0; fi
            j=$((j + 1))
        done
        return 1
    fi
    case ${_pt_s:_pt_i+1:1} in
        '(')
            depth=0; j=$((_pt_i + 1))
            while [ $j -lt $_pt_n ]; do
                case ${_pt_s:j:1} in
                    '(') depth=$((depth + 1)) ;;
                    ')')
                        depth=$((depth - 1))
                        if [ $depth -le 0 ]; then _pt_i=$((j + 1)); return 0; fi ;;
                esac
                j=$((j + 1))
            done
            return 1 ;;
        '{')
            j=$((_pt_i + 2))
            while [ $j -lt $_pt_n ]; do
                if [ "${_pt_s:j:1}" = '}' ]; then _pt_i=$((j + 1)); return 0; fi
                j=$((j + 1))
            done
            return 1 ;;
        *)
            j=$((_pt_i + 1))
            while [ $j -lt $_pt_n ]; do
                case ${_pt_s:j:1} in
                    [A-Za-z0-9_]) j=$((j + 1)) ;;
                    *) break ;;
                esac
            done
            if [ $j -eq $((_pt_i + 1)) ]; then j=$((_pt_i + 2)); fi
            _pt_i=$j
            return 0 ;;
    esac
}

_policy_scan_redirect() {
    if [ "$_pt_have" = 1 ] && [ "$_pt_dyn" = 0 ] && [[ $_pt_word =~ ^[0-9]+$ ]]; then
        _pt_word=""; _pt_have=0
    fi
    _policy_flush_word
    if [ "${_pt_s:_pt_i:1}" = '<' ] && [ "${_pt_s:_pt_i+1:1}" = '<' ] \
        && [ "${_pt_s:_pt_i+2:1}" != '<' ]; then
        _pt_i=$((_pt_i + 2))
        _pt_hdstrip=0
        if [ "${_pt_s:_pt_i:1}" = '-' ]; then _pt_hdstrip=1; _pt_i=$((_pt_i + 1)); fi
        while [ "${_pt_s:_pt_i:1}" = ' ' ] || [ "${_pt_s:_pt_i:1}" = $'\t' ]; do
            _pt_i=$((_pt_i + 1))
        done
        _pt_hdpend=1
        return 0
    fi
    _pt_i=$((_pt_i + 1))
    while [ $_pt_i -lt $_pt_n ]; do
        case ${_pt_s:_pt_i:1} in
            '<'|'>'|'&'|'|') _pt_i=$((_pt_i + 1)) ;;
            *) break ;;
        esac
    done
    _pt_discard=1
}

# TODO(jb-comment): why the body is found with one suffix removal rather than a
# line-at-a-time loop, and what a `<<-` body costs instead
_policy_skip_heredocs() {
    local delim strip rest line pre
    while [ ${#_pt_hd[@]} -gt 0 ]; do
        delim=${_pt_hd[0]}
        _pt_hd=("${_pt_hd[@]:1}")
        strip=0
        case $delim in
            $'\x01'*) strip=1; delim=${delim#$'\x01'} ;;
        esac
        if [ $_pt_i -ge $_pt_n ]; then continue; fi
        rest=${_pt_s:_pt_i}
        if [ "$strip" = 0 ]; then
            case $rest in
                "$delim"$'\n'*) _pt_i=$((_pt_i + ${#delim} + 1)); continue ;;
                "$delim")       _pt_i=$_pt_n; continue ;;
            esac
            pre=${rest%%$'\n'"$delim"$'\n'*}
            if [ "$pre" != "$rest" ]; then
                _pt_i=$((_pt_i + ${#pre} + ${#delim} + 2))
                continue
            fi
            pre=${rest%$'\n'"$delim"}
            _pt_i=$_pt_n
            continue
        fi
        while : ; do
            if [ $_pt_i -ge $_pt_n ]; then break; fi
            rest=${_pt_s:_pt_i}
            line=${rest%%$'\n'*}
            if [ "$line" = "$rest" ]; then
                _pt_i=$_pt_n
            else
                _pt_i=$((_pt_i + ${#line} + 1))
            fi
            while [ "${line:0:1}" = $'\t' ]; do line=${line:1}; done
            if [ "$line" = "$delim" ]; then break; fi
        done
    done
}

policy_tokenize() {
    local _pt_s=${1-}
    local _pt_n=${#_pt_s}
    local _pt_i=0
    local _pt_word="" _pt_have=0 _pt_dyn=0
    local _pt_cur="" _pt_curn=0
    local _pt_discard=0 _pt_hdpend=0 _pt_hdstrip=0
    local -a _pt_hd=()
    local c rest pre

    POLICY_SEGMENTS=()

    while [ $_pt_i -lt $_pt_n ]; do
        c=${_pt_s:_pt_i:1}
        case $c in
            '\')
                if [ "${_pt_s:_pt_i+1:1}" = $'\n' ]; then
                    _pt_i=$((_pt_i + 2))
                else
                    _pt_word+=${_pt_s:_pt_i+1:1}; _pt_have=1; _pt_i=$((_pt_i + 2))
                fi ;;
            "'")       _policy_scan_squote    || return 1 ;;
            '"')       _policy_scan_dquote    || return 1 ;;
            '$'|'`')   _policy_scan_expansion || return 1 ;;
            ' '|$'\t') _policy_flush_word; _pt_i=$((_pt_i + 1)) ;;
            $'\n')
                _policy_flush_word
                _pt_i=$((_pt_i + 1))
                if [ ${#_pt_hd[@]} -gt 0 ]; then _policy_skip_heredocs; fi
                _policy_flush_segment ;;
            ';'|'&'|'|')
                _policy_flush_segment
                while [ $_pt_i -lt $_pt_n ]; do
                    case ${_pt_s:_pt_i:1} in
                        ';'|'&'|'|') _pt_i=$((_pt_i + 1)) ;;
                        *) break ;;
                    esac
                done ;;
            '('|')')   _policy_flush_segment; _pt_i=$((_pt_i + 1)) ;;
            '<'|'>')   _policy_scan_redirect ;;
            '#')
                if [ "$_pt_have" = 1 ]; then
                    _pt_word+=$c; _pt_i=$((_pt_i + 1))
                else
                    rest=${_pt_s:_pt_i}
                    pre=${rest%%$'\n'*}
                    if [ "$pre" = "$rest" ]; then
                        _pt_i=$_pt_n
                    else
                        _pt_i=$((_pt_i + ${#pre}))
                    fi
                fi ;;
            *) _pt_word+=$c; _pt_have=1; _pt_i=$((_pt_i + 1)) ;;
        esac
    done

    _policy_flush_segment
    return 0
}

# policy_seg_words SEGMENT -> POLICY_WORDS[] POLICY_LIT[]
policy_seg_words() {
    local rest=${1-} w
    POLICY_WORDS=()
    POLICY_LIT=()
    while [ -n "$rest" ]; do
        case $rest in
            *$'\x1f'*) ;;
            *) break ;;
        esac
        w=${rest%%$'\x1f'*}
        rest=${rest#*$'\x1f'}
        POLICY_WORDS+=("${w:1}")
        POLICY_LIT+=("${w:0:1}")
    done
}

# policy_segments CMD -> POLICY_SEGMENTS[], with `sh -c '<literal>'` replaced
# by the segments of the literal.
# TODO(jb-comment): why only a literal -c argument is unwrapped, and why the
# recursion stops at one level
policy_segments() {
    policy_tokenize "${1-}" || return 1
    local -a out=()
    local -a keep=()
    local seg inner cmd i n
    for seg in "${POLICY_SEGMENTS[@]}"; do
        policy_seg_words "$seg"
        n=${#POLICY_WORDS[@]}
        cmd=""
        if [ $n -gt 0 ] && [ "${POLICY_LIT[0]}" = l ]; then
            cmd=${POLICY_WORDS[0]##*/}
        fi
        inner=""
        case $cmd in
            bash|sh|zsh|dash|ksh)
                i=1
                while [ $i -lt $n ]; do
                    if [ "${POLICY_WORDS[i]}" = "-c" ] && [ $((i + 1)) -lt $n ] \
                        && [ "${POLICY_LIT[i+1]}" = l ]; then
                        inner=${POLICY_WORDS[i+1]}
                        break
                    fi
                    i=$((i + 1))
                done ;;
        esac
        if [ -n "$inner" ]; then
            keep=("${out[@]}")
            if policy_tokenize "$inner"; then
                out=("${keep[@]}" "${POLICY_SEGMENTS[@]}")
            else
                out=("${keep[@]}" "$seg")
            fi
        else
            out+=("$seg")
        fi
    done
    POLICY_SEGMENTS=("${out[@]}")
    return 0
}

# --- Gated command classification --------------------------------------------
# policy_classify_command CMD
#   POLICY_VERDICT    allow | gate | chain | nofile | bare
#   POLICY_KIND       label for the text being written
#   POLICY_FILEFLAG   flag the command must be re-issued with
#   POLICY_TEMPLATE   commit | markdown | pr | titled
#   POLICY_NEEDS_AI   the buffer carries a mandatory `## AI use` section
#   POLICY_FILE_ARG   file the command already points at, when literal

policy_classify_command() {
    POLICY_VERDICT=allow
    POLICY_KIND=""
    POLICY_FILEFLAG=""
    POLICY_TEMPLATE=markdown
    POLICY_NEEDS_AI=0
    POLICY_FILE_ARG=""

    policy_segments "${1-}" || return 0

    local total=${#POLICY_SEGMENTS[@]} hits=0 seg
    local -a segs=("${POLICY_SEGMENTS[@]}")
    for seg in "${segs[@]}"; do
        policy_seg_words "$seg"
        if _policy_classify_segment; then hits=$((hits + 1)); fi
    done

    if [ $hits -gt 0 ] && [ $total -gt 1 ]; then
        POLICY_VERDICT=chain
    fi
    return 0
}

_policy_classify_segment() {
    [ ${#POLICY_WORDS[@]} -gt 0 ] || return 1
    [ "${POLICY_LIT[0]}" = l ] || return 1
    case ${POLICY_WORDS[0]##*/} in
        git) _policy_classify_git ;;
        gh)  _policy_classify_gh ;;
        *)   return 1 ;;
    esac
}

_policy_classify_git() {
    local n=${#POLICY_WORDS[@]} i=1 sub=""
    while [ $i -lt $n ]; do
        [ "${POLICY_LIT[i]}" = l ] || return 1
        case ${POLICY_WORDS[i]} in
            -C|-c|--git-dir|--work-tree|--namespace|--exec-path|--super-prefix|--config-env)
                i=$((i + 2)) ;;
            -*) i=$((i + 1)) ;;
            *)  sub=${POLICY_WORDS[i]}; i=$((i + 1)); break ;;
        esac
    done
    [ -n "$sub" ] || return 1

    case $sub in
        commit) _policy_git_commit "$i" ;;
        merge)
            _policy_git_message "$i" || return 1
            _policy_gate "merge commit message" "-F" commit 0
            return 0 ;;
        tag)
            if ! _policy_git_message "$i"; then
                _policy_git_short "$i" a || _policy_git_long "$i" --annotate || return 1
            fi
            _policy_gate "tag message" "-F" commit 0
            return 0 ;;
        notes)
            _policy_git_message "$i" || return 1
            _policy_gate "git note" "-F" commit 0
            return 0 ;;
        *) return 1 ;;
    esac
}

_policy_gate() {
    POLICY_VERDICT=gate
    POLICY_KIND=$1
    POLICY_FILEFLAG=$2
    POLICY_TEMPLATE=$3
    POLICY_NEEDS_AI=$4
}

# TODO(jb-comment): why bundled short options are walked letter by letter and
# stopped at the first one that consumes a value
_policy_git_walk() {
    _POLICY_GIT_MSG=0
    _POLICY_GIT_EXEMPT=0
    _POLICY_GIT_AUTOMSG=0
    _POLICY_GIT_FILE=""
    _POLICY_GIT_FILELIT=d
    local i=$1 n=${#POLICY_WORDS[@]} w rest ch
    while [ $i -lt $n ]; do
        w=${POLICY_WORDS[i]}
        case $w in
            --) break ;;
            --message=*)                       _POLICY_GIT_MSG=1 ;;
            --message)                         _POLICY_GIT_MSG=1; i=$((i + 1)) ;;
            --no-edit)                         _POLICY_GIT_EXEMPT=1 ;;
            --reuse-message=*)                 _POLICY_GIT_EXEMPT=1 ;;
            --reuse-message)                   _POLICY_GIT_EXEMPT=1; i=$((i + 1)) ;;
            --fixup=*|--squash=*)              _POLICY_GIT_AUTOMSG=1 ;;
            --fixup|--squash)                  _POLICY_GIT_AUTOMSG=1; i=$((i + 1)) ;;
            --file=*)
                _POLICY_GIT_FILE=${w#*=}; _POLICY_GIT_FILELIT=${POLICY_LIT[i]} ;;
            --file)
                if [ $((i + 1)) -lt $n ]; then
                    _POLICY_GIT_FILE=${POLICY_WORDS[i+1]}
                    _POLICY_GIT_FILELIT=${POLICY_LIT[i+1]}
                fi
                i=$((i + 1)) ;;
            --template|--author|--date|--cleanup|--gpg-sign|--trailer|--pathspec-from-file)
                i=$((i + 1)) ;;
            --*) ;;
            -?*)
                rest=${w#-}
                while [ -n "$rest" ]; do
                    ch=${rest:0:1}; rest=${rest:1}
                    case $ch in
                        m)
                            _POLICY_GIT_MSG=1
                            if [ -z "$rest" ]; then i=$((i + 1)); fi
                            break ;;
                        C)
                            _POLICY_GIT_EXEMPT=1
                            if [ -z "$rest" ]; then i=$((i + 1)); fi
                            break ;;
                        F)
                            if [ -n "$rest" ]; then
                                _POLICY_GIT_FILE=$rest
                                _POLICY_GIT_FILELIT=${POLICY_LIT[i]}
                            elif [ $((i + 1)) -lt $n ]; then
                                _POLICY_GIT_FILE=${POLICY_WORDS[i+1]}
                                _POLICY_GIT_FILELIT=${POLICY_LIT[i+1]}
                                i=$((i + 1))
                            fi
                            break ;;
                        c|t)
                            if [ -z "$rest" ]; then i=$((i + 1)); fi
                            break ;;
                        S|u) break ;;
                        *) ;;
                    esac
                done ;;
            *) ;;
        esac
        i=$((i + 1))
    done
}

_policy_git_message() {
    _policy_git_walk "$1"
    [ "$_POLICY_GIT_MSG" = 1 ]
}

_policy_git_short() {
    local i=$1 letter=$2 n=${#POLICY_WORDS[@]} w
    while [ $i -lt $n ]; do
        w=${POLICY_WORDS[i]}
        case $w in
            --) return 1 ;;
            --*) ;;
            -?*) case ${w#-} in *"$letter"*) return 0 ;; esac ;;
        esac
        i=$((i + 1))
    done
    return 1
}

_policy_git_long() {
    local i=$1 flag=$2 n=${#POLICY_WORDS[@]}
    while [ $i -lt $n ]; do
        case ${POLICY_WORDS[i]} in
            "$flag"|"$flag"=*) return 0 ;;
        esac
        i=$((i + 1))
    done
    return 1
}

# TODO(jb-comment): why --fixup and --squash are allowed through where the
# substring gate opened a buffer that could not be used
_policy_git_commit() {
    _policy_git_walk "$1"
    if [ "$_POLICY_GIT_EXEMPT" = 1 ] || [ "$_POLICY_GIT_AUTOMSG" = 1 ]; then
        return 1
    fi
    if [ -n "$_POLICY_GIT_FILE" ]; then
        _policy_gate "commit message" "-F" commit 0
        if [ "$_POLICY_GIT_FILELIT" = l ]; then POLICY_FILE_ARG=$_POLICY_GIT_FILE; fi
        return 0
    fi
    if [ "$_POLICY_GIT_MSG" = 1 ]; then
        _policy_gate "commit message" "-F" commit 0
        return 0
    fi
    _policy_gate "commit message" "-F" commit 0
    POLICY_VERDICT=bare
    return 0
}

_policy_gh_scan() {
    _POLICY_GH_BODY=0
    _POLICY_GH_COMMENT=0
    _POLICY_GH_FILE=""
    _POLICY_GH_FILELIT=d
    local i=3 n=${#POLICY_WORDS[@]} w
    while [ $i -lt $n ]; do
        w=${POLICY_WORDS[i]}
        case $w in
            --) break ;;
            --body|-b)        _POLICY_GH_BODY=1; i=$((i + 1)) ;;
            --body=*)         _POLICY_GH_BODY=1 ;;
            --comment|-c)     _POLICY_GH_COMMENT=1; i=$((i + 1)) ;;
            --comment=*)      _POLICY_GH_COMMENT=1 ;;
            --body-file|--notes-file|-F)
                if [ $((i + 1)) -lt $n ]; then
                    _POLICY_GH_FILE=${POLICY_WORDS[i+1]}
                    _POLICY_GH_FILELIT=${POLICY_LIT[i+1]}
                fi
                i=$((i + 1)) ;;
            --body-file=*|--notes-file=*)
                _POLICY_GH_FILE=${w#*=}; _POLICY_GH_FILELIT=${POLICY_LIT[i]} ;;
            --title|-t|--repo|-R|--base|-B|--head|-H|--assignee|-a|--label|-l)
                i=$((i + 1)) ;;
            --milestone|-m|--project|-p|--reviewer|-r|--notes|-n|--tag|--target)
                i=$((i + 1)) ;;
            *) ;;
        esac
        i=$((i + 1))
    done
}

# TODO(jb-comment): why gh api is classified only on a write, and why -m here
# is a milestone rather than a message
_policy_gh_api_writes() {
    local i=2 n=${#POLICY_WORDS[@]} w
    while [ $i -lt $n ]; do
        w=${POLICY_WORDS[i]}
        case $w in
            --input|--input=*) return 0 ;;
            -f|-F|--field|--raw-field)
                if [ $((i + 1)) -lt $n ]; then
                    case ${POLICY_WORDS[i+1]} in body=*) return 0 ;; esac
                fi
                i=$((i + 1)) ;;
            --field=*|--raw-field=*) case ${w#*=} in body=*) return 0 ;; esac ;;
            -f*|-F*) case ${w#-?} in body=*) return 0 ;; esac ;;
        esac
        i=$((i + 1))
    done
    return 1
}

_policy_classify_gh() {
    local n=${#POLICY_WORDS[@]}
    [ $n -ge 2 ] || return 1
    [ "${POLICY_LIT[1]}" = l ] || return 1
    local s1=${POLICY_WORDS[1]} s2=""
    if [ $n -ge 3 ] && [ "${POLICY_LIT[2]}" = l ]; then s2=${POLICY_WORDS[2]}; fi

    local kind="" flag="" tmpl=markdown ai=0 need=""
    case "$s1 $s2" in
        "pr create")     kind="pull request";        flag="--body-file"; ai=1; tmpl=pr ;;
        "pr edit")       kind="pull request body";   flag="--body-file"; ai=1; tmpl=pr; need=body ;;
        "pr comment")    kind="PR comment";          flag="--body-file" ;;
        "pr review")     kind="PR review";           flag="--body-file" ;;
        "issue create")  kind="issue";               flag="--body-file"; tmpl=titled ;;
        "issue edit")    kind="issue body";          flag="--body-file"; need=body ;;
        "issue comment") kind="issue comment";       flag="--body-file" ;;
        "issue close")   kind="issue closing comment"; flag=""; need=comment ;;
        "release create") kind="release notes";      flag="--notes-file" ;;
        "gist create")   kind="gist";                flag="" ;;
        *)
            if [ "$s1" = api ]; then
                _policy_gh_api_writes || return 1
                POLICY_VERDICT=nofile
                POLICY_KIND="GitHub API post"
                return 0
            fi
            return 1 ;;
    esac

    _policy_gh_scan
    case $need in
        body)
            if [ "$_POLICY_GH_BODY" != 1 ] && [ -z "$_POLICY_GH_FILE" ]; then
                return 1
            fi ;;
        comment)
            [ "$_POLICY_GH_COMMENT" = 1 ] || return 1 ;;
    esac

    if [ -z "$flag" ]; then
        POLICY_VERDICT=nofile
        POLICY_KIND=$kind
        return 0
    fi

    _policy_gate "$kind" "$flag" "$tmpl" "$ai"
    if [ "$_POLICY_GH_FILELIT" = l ]; then POLICY_FILE_ARG=$_POLICY_GH_FILE; fi
    return 0
}
