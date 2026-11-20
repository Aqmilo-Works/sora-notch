//! What a copied text is: a link (with or without https://), an email, a colour, a phone number, code (and which
//! language), or plain words. Used by the Board (board.rs). Everything here is a guess from the text alone, so each
//! rule is written to be wrong on the side of "text": a missed link or snippet is cheaper than words filed as code.

const MAX_LINK: usize = 2048;

/// Every top-level domain there is (IANA's list, lowercase): any of them makes a bare `name.tld` a web address (`kompas.id`, `web.dev`).
/// shortcut: a copy of the list, refresh src/tlds.txt from data.iana.org when a new ending is missed.
const TLDS: &str = include_str!("tlds.txt");
fn real_tld(t: &str) -> bool { TLDS.lines().any(|l| l == t) }
/// Endings that are also file extensions count only when a path follows (`docs.rs/serde`): `README.md`, `main.rs` and `libc.so` are files.
const TLD_PATH_ONLY: &[&str] = &["rs", "sh", "py", "md", "pl", "cc", "zip", "mov", "ts", "cs", "js", "rb", "go", "so", "ps",
    "java", "map", "mk", "am", "ml", "mm", "tf"];
/// Names that are variables in code, not sites: `user.id`, `item.name` and `hello.world` stay text unless `www.` or a path makes them links.
const CODE_NAMES: &[&str] = &["user", "users", "item", "items", "obj", "this", "self", "data", "props", "state", "config", "req", "res", "ctx", "event", "evt",
    "el", "node", "row", "args", "opts", "options", "settings", "result", "error", "err", "response", "request", "window", "document", "console", "process",
    "module", "exports", "hello", "foo", "bar", "baz", "model", "record", "entry", "person", "member", "file", "params", "query", "body", "payload", "msg", "message"];

/// A link, with or without `https://`: `https://a.b/c`, `www.a.com`, `github.com/user/repo`. Gives the address as a browser
/// needs it (https:// added when it was left out), or None when the text is not one word shaped like an address.
pub fn link_url(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.len() > MAX_LINK || t.contains(char::is_whitespace) { return None; }
    if t.ends_with([',', ';', ')', ']', '"', '\'', '.']) { return None; }
    let lower = t.to_ascii_lowercase();
    for scheme in ["http://", "https://"] {
        if lower.starts_with(scheme) { return (t.len() > scheme.len() && !t[scheme.len()..].starts_with('/')).then(|| t.to_string()); }
    }
    if t.contains("://") { return None; }
    let end = t.find(['/', '?', '#']).unwrap_or(t.len());
    let (host_port, rest) = t.split_at(end);
    if host_port.contains('@') { return None; }
    let (host, has_port) = match host_port.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.len() <= 5 && p.chars().all(|c| c.is_ascii_digit()) => (h, true),
        _ => (host_port, false),
    };
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || host.contains("..") { return None; }
    let labels: Vec<&str> = host.split('.').collect();
    if !labels.iter().all(|l| !l.is_empty() && l.len() <= 63 && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !l.starts_with('-') && !l.ends_with('-')) { return None; }
    let h = host.to_ascii_lowercase();
    if h == "localhost" { return has_port.then(|| format!("http://{t}")); }
    if labels.len() < 2 { return None; }
    let tld = labels[labels.len() - 1].to_ascii_lowercase();
    let tld = tld.as_str();
    let has_path = !rest.is_empty();
    let www = h.starts_with("www.") && labels.len() >= 3;
    // A property chain in code: a known variable name (user.id) or a camelCase one (userProfile.name).
    let first = labels[0];
    let code = labels.len() == 2 && (CODE_NAMES.contains(&first.to_ascii_lowercase().as_str()) || first.as_bytes().windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase()));
    let ok = www || if TLD_PATH_ONLY.contains(&tld) { has_path } else { real_tld(tld) && (has_path || !code) };
    ok.then(|| format!("https://{t}"))
}

pub fn classify(text: &str) -> &'static str {
    let t = text.trim();
    if link_url(t).is_some() { return "link"; }
    let one_line = !t.contains('\n') && !t.contains(char::is_whitespace);
    if one_line && t.len() <= 254 {
        if let Some((user, host)) = t.split_once('@') {
            if !user.is_empty() && host.contains('.') && !host.starts_with('.') && !host.ends_with('.') && !host.contains('@') && t.chars().all(|c| c.is_ascii_alphanumeric() || "@._+-%".contains(c)) { return "email"; }
        }
        let hex = t.strip_prefix('#').filter(|h| matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit()));
        // A bare hex (FFF2A5) counts only with both a digit and a letter, so words (facade) and numbers (123456) stay what they are.
        let bare = matches!(t.len(), 6 | 8) && t.chars().all(|c| c.is_ascii_hexdigit()) && t.chars().any(|c| c.is_ascii_digit()) && t.chars().any(|c| c.is_ascii_alphabetic());
        if hex.is_some() || bare { return "color"; }
    }
    // rgb(79, 157, 255) and hsl(213, 100%, 65%) have spaces, so they are looked at apart from the one-word forms.
    if !t.contains('\n') && t.len() <= 64 && (t.starts_with("rgb") || t.starts_with("hsl")) && t.ends_with(')') { return "color"; }
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    if t.len() <= 22 && (8..=15).contains(&digits) && t.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c)) { return "phone"; }
    if is_code(t) { return "code"; }
    "text"
}

pub fn is_code(t: &str) -> bool { code_language(t).is_some() }

// ---- code: the language, when it can be told. "Code" alone (an empty name) is a block that only looks like code.
struct Lang { name: &'static str, nocase: bool, pats: &'static [(&'static str, u8)] }
// A pattern starting with ^ must begin a line (after its indent); the others may be anywhere. Weights: 1 weak, 2 fair, 3 sure.
// A language needs 3 in all (plus 1 when the text has the marks of code: braces, semicolons, arrows).
const LANGS: &[Lang] = &[
    Lang { name: "Rust", nocase: false, pats: &[("^fn ", 2), ("fn main()", 3), ("^let mut ", 3), ("^impl ", 2), ("^pub fn ", 3), ("^pub struct ", 3), ("^use std::", 3), ("println!", 3), ("#[derive", 3), ("&self", 2), ("&mut ", 2), ("Vec<", 2), ("Option<", 2), ("Result<", 2), (".unwrap()", 2), ("crate::", 2), ("::new(", 1), ("^mod ", 1), ("^enum ", 1), ("^match ", 1), ("-> ", 1), ("::<", 2), ("unsafe {", 2), ("^let ", 1)] },
    Lang { name: "Go", nocase: false, pats: &[("^func ", 3), ("^package ", 3), (":= ", 2), ("fmt.", 3), ("go func", 3), ("^defer ", 3), ("chan ", 2), ("^import (", 3), ("err != nil", 3), ("^type ", 1), ("struct {", 2), ("interface {", 2), ("make(", 1), ("[]string", 2), ("[]int", 2)] },
    Lang { name: "Python", nocase: false, pats: &[("^def ", 3), ("^async def ", 3), ("^class ", 1), ("^import ", 1), ("^from ", 1), (" import ", 1), ("self.", 2), ("(self", 3), ("^elif ", 3), ("elif ", 2), ("print(", 2), ("__name__", 3), ("__init__", 3), ("lambda ", 2), ("^try:", 3), ("^except", 2), ("^with ", 1), ("^for ", 1), ("^while ", 1), ("range(", 2), ("len(", 1), ("f\"", 1), ("f'", 1), ("None", 1), ("^raise ", 2), ("^@", 1), ("^pass", 2), ("^return ", 1), ("^if ", 1), ("):", 1), ("pip install", 2)] },
    Lang { name: "JavaScript", nocase: false, pats: &[("^const ", 2), ("^let ", 1), ("^var ", 1), ("function ", 1), ("function(", 2), ("=> ", 2), ("=>{", 2), ("console.log", 3), ("console.", 2), ("require(", 3), ("^import ", 1), (" from '", 2), (" from \"", 2), ("^export ", 2), ("export default", 3), ("module.exports", 3), ("document.", 2), ("window.", 2), ("async ", 1), ("await ", 2), ("===", 2), ("!==", 2), ("useState", 3), ("useEffect", 3), ("JSON.parse", 3), ("JSON.stringify", 3), ("addEventListener", 3), ("new Promise", 3), (".then(", 2), (".map(", 2), (".filter(", 2), (".forEach(", 2), ("${", 1), (" = ", 1), ("null", 1), ("undefined", 2)] },
    Lang { name: "TypeScript", nocase: false, pats: &[("^interface ", 3), ("^export interface ", 3), ("^type ", 1), (": string", 2), (": number", 2), (": boolean", 2), ("<T>", 3), ("^enum ", 1), ("readonly ", 2), ("as const", 3), ("^import type", 3), ("Promise<", 3), ("?: ", 2), (": any", 3), (": void", 2), ("<string>", 3), ("implements ", 1), ("^declare ", 3)] },
    Lang { name: "Java", nocase: false, pats: &[("public class ", 3), ("public static void main", 3), ("System.out.", 3), ("^private ", 2), ("^public ", 2), ("^protected ", 2), ("@Override", 3), ("^import java.", 3), ("^package ", 1), (" extends ", 1), (" implements ", 2), ("new ", 1), ("String[]", 2), ("List<", 1), ("throws ", 2), ("final ", 1), ("void ", 1)] },
    Lang { name: "C#", nocase: false, pats: &[("^using System", 3), ("^namespace ", 2), ("Console.Write", 3), ("static void Main", 3), ("{ get; set; }", 3), ("async Task", 3), ("public class ", 1), ("^using ", 1), ("string[]", 2), ("var ", 1), ("[HttpGet", 3), ("List<", 1), ("foreach (", 2)] },
    Lang { name: "C++", nocase: false, pats: &[("#include <", 3), ("std::", 3), ("cout <<", 3), ("cin >>", 3), ("template<", 3), ("template <", 3), ("nullptr", 3), ("using namespace", 3), ("::", 1), ("vector<", 3), ("int main(", 2), ("class ", 1), ("->", 1), ("endl", 2)] },
    Lang { name: "C", nocase: false, pats: &[("#include", 3), ("int main(", 3), ("printf(", 3), ("scanf(", 3), ("malloc(", 3), ("free(", 2), ("sizeof(", 3), ("#define ", 3), ("typedef ", 2), ("struct ", 1), ("void ", 1), ("char *", 2), ("NULL", 1), ("return 0;", 3), ("int ", 1)] },
    Lang { name: "PHP", nocase: false, pats: &[("<?php", 3), ("$this->", 3), ("$_GET", 3), ("$_POST", 3), ("^echo ", 1), ("^namespace ", 1), ("^use ", 1), ("->", 1), ("=> ", 1), ("function ", 1), ("$", 1), ("array(", 2), ("public function", 3), ("::class", 2)] },
    Lang { name: "Ruby", nocase: false, pats: &[("^def ", 2), ("^end", 2), ("^puts ", 2), ("^require '", 3), ("^require \"", 3), ("attr_accessor", 3), ("do |", 3), (".each do", 3), ("^class ", 1), (" < ", 1), ("@", 1), ("^module ", 1), ("^elsif ", 3), ("unless ", 2), ("nil", 1), ("=> ", 1), ("end\n", 1)] },
    Lang { name: "Swift", nocase: false, pats: &[("^import UIKit", 3), ("^import SwiftUI", 3), ("^import Foundation", 3), ("^func ", 2), ("^let ", 1), ("^var ", 1), ("guard let", 3), ("@State", 3), (": View", 3), ("-> ", 1), ("if let ", 3), ("@objc", 3), ("print(", 1), ("override func", 3), ("some View", 3)] },
    Lang { name: "Kotlin", nocase: false, pats: &[("^fun ", 3), ("^val ", 2), ("^var ", 1), ("println(", 2), ("data class", 3), ("override fun", 3), ("companion object", 3), ("suspend fun", 3), ("?.", 1), ("?:", 1), ("^package ", 1), ("^import ", 1), (" is ", 0), ("when (", 2), ("lateinit", 3)] },
    Lang { name: "Scala", nocase: false, pats: &[("^def ", 1), ("^val ", 1), ("^object ", 2), ("case class", 3), ("^trait ", 3), ("extends App", 3), ("implicit ", 3), ("=> ", 1), (" match {", 3), ("println(", 1), ("^import scala", 3)] },
    Lang { name: "Dart", nocase: false, pats: &[("void main()", 3), ("import 'package:", 3), ("Widget build(", 3), ("StatelessWidget", 3), ("StatefulWidget", 3), ("setState(", 3), ("^final ", 1), ("@override", 2), ("print(", 1), ("async {", 1), ("Future<", 3)] },
    Lang { name: "Lua", nocase: false, pats: &[("^local ", 2), ("^function ", 1), ("^end", 1), (" then", 1), ("require(", 1), ("print(", 1), ("~=", 3), ("^elseif ", 3), ("ipairs(", 3), ("pairs(", 2), ("nil", 1), ("-- ", 1), ("local function", 3)] },
    Lang { name: "Perl", nocase: false, pats: &[("^my $", 3), ("^my @", 3), ("^my %", 3), ("use strict", 3), ("use warnings", 3), ("^sub ", 3), ("=~", 3), ("$_", 2), ("@_", 3), ("print \"", 1), ("->", 1), ("::", 1)] },
    Lang { name: "R", nocase: false, pats: &[("<- ", 2), ("library(", 3), ("function(", 1), ("data.frame", 3), ("%>%", 3), ("ggplot(", 3), ("c(", 1), ("$", 1), ("NULL", 1), ("TRUE", 1), ("print(", 1), ("install.packages", 3), ("|>", 1)] },
    Lang { name: "Julia", nocase: false, pats: &[("^using ", 2), ("^function ", 1), ("^end", 1), ("println(", 1), ("::Int", 3), ("::Float", 3), ("^module ", 1), ("@time", 3), (".+", 1), ("using LinearAlgebra", 3)] },
    Lang { name: "Haskell", nocase: false, pats: &[("^module ", 2), ("^import qualified", 3), (":: ", 2), (" where", 1), ("main = do", 3), ("^data ", 1), ("^instance ", 3), (" <- ", 1), ("^let ", 1), ("-> ", 1), ("putStrLn", 3), ("::", 1), ("{-#", 3)] },
    Lang { name: "Elixir", nocase: false, pats: &[("defmodule ", 3), ("^def ", 1), ("^defp ", 3), ("|> ", 3), ("IO.puts", 3), ("^end", 1), (" do\n", 2), (" do", 1), ("@spec", 3), ("fn ", 1), ("%{", 2), ("^use ", 1)] },
    Lang { name: "Erlang", nocase: false, pats: &[("-module(", 3), ("-export(", 3), ("io:format", 3), ("-spec ", 3), ("-> ", 1), ("-record(", 3), ("fun(", 2)] },
    Lang { name: "Clojure", nocase: false, pats: &[("(defn ", 3), ("(def ", 3), ("(ns ", 3), ("(let [", 3), ("(println ", 2), ("(fn [", 3), ("(-> ", 2), ("(:require", 3)] },
    Lang { name: "Solidity", nocase: false, pats: &[("pragma solidity", 3), ("^contract ", 3), ("msg.sender", 3), ("mapping(", 3), ("uint256", 3), ("address ", 1), ("require(", 1), ("emit ", 2), ("function ", 1), ("public ", 1), ("external", 1)] },
    Lang { name: "PowerShell", nocase: true, pats: &[("get-", 2), ("set-", 2), ("new-item", 3), ("remove-item", 3), ("invoke-", 3), ("write-host", 3), ("write-output", 3), ("$env:", 3), ("| where-object", 3), ("| foreach-object", 3), ("| select-object", 3), ("param(", 2), ("^iwr ", 3), ("^irm ", 3), ("| iex", 3), ("iex ", 2), ("-erroraction", 3), ("$psscriptroot", 3), ("[cmdletbinding", 3), ("start-process", 3), ("-useb", 3), ("^foreach ($", 3), ("$_", 1), ("-eq ", 1), ("-ne ", 1)] },
    Lang { name: "Batch", nocase: true, pats: &[("@echo off", 3), ("^set /p", 3), ("^goto ", 2), ("%~dp0", 3), ("^if exist", 3), ("^rem ", 2), ("^setlocal", 3), ("^pause", 1), ("%errorlevel%", 3), ("^call ", 1), ("^echo.", 2), ("^cls", 2)] },
    Lang { name: "Shell", nocase: false, pats: &[("#!/bin/", 3), ("#!/usr/bin/env", 3), ("^if [", 3), ("^fi", 2), ("^then", 1), ("$(", 2), ("^export ", 2), ("^sudo ", 3), ("^apt ", 3), ("^apt-get ", 3), ("chmod +x", 3), ("| grep", 3), ("| bash", 3), ("| sh", 3), (" && ", 1), ("${", 2), ("set -e", 3), ("^source ", 2), ("^echo ", 1), ("^cd ", 1), ("2>&1", 3), ("/dev/null", 3), ("^for ", 1), ("^done", 2), ("^esac", 3), ("$@", 2), ("$1", 1), ("curl ", 2), ("wget ", 2)] },
    Lang { name: "SQL", nocase: true, pats: &[("^select ", 2), (" from ", 1), (" where ", 1), ("insert into", 3), ("create table", 3), ("^update ", 1), (" set ", 1), ("delete from", 3), (" join ", 2), ("group by", 3), ("order by", 3), ("alter table", 3), ("drop table", 3), ("primary key", 3), ("varchar(", 3), ("count(*)", 3), ("^with ", 1), (" inner join", 3), ("limit ", 1), ("create index", 3)] },
    Lang { name: "HTML", nocase: true, pats: &[("<!doctype html", 3), ("<html", 3), ("<div", 2), ("</div>", 3), ("<script", 3), ("<body", 3), ("<head", 3), ("<a href", 3), ("<span", 2), ("<p>", 2), ("<img ", 2), ("<ul>", 3), ("<li>", 3), ("<button", 2), ("<input", 2), ("<form", 3), ("<table", 3), ("class=\"", 2), ("<h1", 2), ("<br", 1), ("</", 1), ("<style", 3), ("<link ", 2), ("<meta ", 3)] },
    Lang { name: "XML", nocase: true, pats: &[("<?xml", 3), ("xmlns", 3), ("<![cdata[", 3), ("<project", 1), ("<dependency>", 3), ("<configuration", 2), ("/>", 1), ("</", 1), ("<manifest", 3), ("<resources", 3)] },
    Lang { name: "CSS", nocase: true, pats: &[("@media", 3), ("@import url", 3), (":root", 3), ("display:", 2), ("margin:", 2), ("padding:", 2), ("color:", 1), ("background:", 2), ("font-size:", 3), ("font-family:", 3), ("border-radius:", 3), ("flex", 1), ("px;", 2), ("{\n", 1), ("@keyframes", 3), (":hover", 2), ("@mixin", 3), ("@include", 3), ("!important", 3), ("rgba(", 1), ("var(--", 3), ("grid-template", 3), ("z-index:", 3), ("position:", 2), ("transition:", 3)] },
    Lang { name: "Dockerfile", nocase: false, pats: &[("^FROM ", 3), ("^RUN ", 2), ("^COPY ", 2), ("^WORKDIR ", 3), ("^CMD [", 3), ("^ENTRYPOINT", 3), ("^EXPOSE ", 3), ("^ENV ", 2), ("^ARG ", 2), ("^ADD ", 1), ("^USER ", 1), ("^LABEL ", 2)] },
    Lang { name: "GraphQL", nocase: false, pats: &[("^query ", 2), ("^mutation ", 3), ("^type ", 1), ("^schema {", 3), ("^input ", 2), ("^subscription ", 3), ("!]", 2), (": String!", 3), (": ID!", 3), ("^fragment ", 3), ("... on ", 3)] },
    Lang { name: "Diff", nocase: false, pats: &[("diff --git", 3), ("@@ -", 3), ("^+++ ", 3), ("^--- a/", 3), ("^--- ", 1), ("^index ", 1), ("^+", 0)] },
    Lang { name: "Assembly", nocase: true, pats: &[("^mov ", 2), ("^push ", 2), ("^pop ", 2), ("^call ", 1), ("^ret", 2), ("^jmp ", 3), (".section", 3), ("global _start", 3), ("^xor ", 2), ("eax", 2), ("rsp", 2), ("^syscall", 3), ("^int 0x", 3), ("^lea ", 3)] },
    Lang { name: "Markdown code", nocase: false, pats: &[("```", 3)] },
];

/// Words that start a one-line shell command and are hardly ever the start of a sentence.
const SURE_COMMANDS: &[&str] = &[
    "git", "npm", "npx", "pnpm", "yarn", "bun", "deno", "cargo", "rustup", "rustc", "pip", "pip3", "pipx", "uv", "conda", "poetry", "docker", "docker-compose", "podman", "kubectl", "helm", "terraform", "ansible", "sudo", "curl", "wget", "winget", "choco", "scoop",
    "dotnet", "msbuild", "nuget", "apt", "apt-get", "brew", "yum", "dnf", "pacman", "snap", "ssh", "scp", "rsync", "powershell", "pwsh", "node", "python", "python3", "py", "java", "javac", "gcc", "g++", "clang", "cmake", "mvn", "gradle", "composer",
    "artisan", "rails", "bundle", "gem", "flutter", "dart", "tsc", "pytest", "jest", "vite", "webpack", "eslint", "prettier", "ffmpeg", "magick", "nslookup", "ipconfig", "netstat", "taskkill", "tasklist", "systemctl", "chmod", "chown", "mkdir", "gh", "aws", "gcloud", "az", "firebase", "vercel", "netlify", "iwr", "irm", "iex", "cmd",
];
/// Words that are also ordinary English: a line starting with one is a command only with a flag, a path, a pipe or a variable in it.
const MAYBE_COMMANDS: &[&str] = &["cd", "ls", "rm", "cp", "mv", "cat", "grep", "find", "echo", "touch", "make", "go", "start", "type", "copy", "del", "dir", "move", "set", "open", "kill", "head", "tail", "sort", "tar", "zip", "unzip", "man", "ping", "which", "where", "run", "build", "test", "install", "update"];

fn has_marks(t: &str) -> bool {
    t.matches(['{', '}', ';']).count() + t.matches("=>").count() + t.matches("();").count() + t.matches("</").count() + t.matches("():").count() + t.matches("){").count() >= 1
}

fn hit(text: &str, lower: &str, nocase: bool, pat: &str) -> bool {
    let (hay, p) = if nocase { (lower, pat) } else { (text, pat) };
    match p.strip_prefix('^') {
        Some(rest) => hay.lines().any(|l| l.trim_start().starts_with(rest)),
        None => hay.contains(p),
    }
}

/// The shell a one-line command belongs to, if the line is one.
fn command_language(t: &str) -> Option<&'static str> {
    let line = t.strip_prefix("$ ").or_else(|| t.strip_prefix("> ")).or_else(|| t.strip_prefix("# ")).unwrap_or(t).trim_start();
    if line.contains('\n') || line.len() > 400 || line.ends_with('.') { return None; }
    let first = line.split_whitespace().next()?;
    let word = first.trim_end_matches(".exe").to_ascii_lowercase();
    let rest = line[first.len()..].trim();
    // Get-ChildItem, Set-Location, Invoke-WebRequest: Verb-Noun
    let mut parts = first.splitn(2, '-');
    if let (Some(verb), Some(noun)) = (parts.next(), parts.next()) {
        // Only the approved PowerShell verbs: "Real-Time" or "Well-Known" are words.
        const VERBS: &[&str] = &["Get", "Set", "New", "Remove", "Add", "Invoke", "Start", "Stop", "Write", "Read", "Test", "Select", "Where", "Import", "Export", "Copy", "Move", "Clear", "Out", "Format", "Install", "Update", "Enable", "Disable", "Join", "Split", "Compare", "Convert", "Rename", "Restart", "Wait", "Register", "Unregister", "Expand", "Compress", "Resolve", "Push", "Pop", "Enter", "Exit"];
        let camel = noun.chars().skip(1).any(char::is_uppercase);
        if VERBS.contains(&verb) && noun.chars().next().is_some_and(char::is_uppercase) && (camel || rest.starts_with(['-', '$', '|', '"', '\'', '.', '\\', '/', '~'])) { return Some("PowerShell"); }
    }
    if first.starts_with("$env:") || first.starts_with("./") || first.starts_with(".\\") || first.starts_with("~/") { return Some(if first.starts_with("$env:") || first.starts_with(".\\") { "PowerShell" } else { "Shell" }); }
    let shell = if matches!(word.as_str(), "iwr" | "irm" | "iex" | "pwsh" | "powershell") { "PowerShell" } else { "Shell" };
    if SURE_COMMANDS.contains(&word.as_str()) { return if rest.is_empty() && !matches!(word.as_str(), "iwr" | "irm" | "iex") { None } else { Some(shell) }; }
    if MAYBE_COMMANDS.contains(&word.as_str()) && !rest.is_empty() && (rest.starts_with('-') || rest.contains(" -") || rest.contains('/') || rest.contains('\\') || rest.contains('|') || rest.contains('>') || rest.contains('$') || rest.contains('*') || rest.starts_with('~')) { return Some("Shell"); }
    None
}

fn structured_language(t: &str) -> Option<&'static str> {
    let trimmed = t.trim();
    // JSON: an object or array that parses and has something in it.
    if (trimmed.starts_with('{') && trimmed.ends_with('}')) || (trimmed.starts_with('[') && trimmed.ends_with(']')) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let filled = match &v { serde_json::Value::Object(o) => !o.is_empty(), serde_json::Value::Array(a) => !a.is_empty() && (a.iter().any(|x| x.is_object() || x.is_array()) || trimmed.contains('\n')), _ => false };
            if filled { return Some("JSON"); }
        }
    }
    let lines: Vec<&str> = trimmed.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')).collect();
    if lines.len() >= 2 {
        // TOML / INI: [section] with key = value lines
        let sections = lines.iter().filter(|l| { let s = l.trim(); s.starts_with('[') && s.ends_with(']') && !s.contains(' ') }).count();
        let pairs = lines.iter().filter(|l| l.split_once('=').is_some_and(|(k, _)| !k.trim().is_empty() && !k.trim().contains(' ') && !l.contains("=="))).count();
        if sections >= 1 && pairs >= 1 { return Some("TOML / INI"); }
        // YAML: key: value lines (or a list), at least three, and nothing that reads as a sentence
        let keys = lines.iter().filter(|l| { let s = l.trim_start(); let s = s.strip_prefix("- ").unwrap_or(s); s.split_once(':').is_some_and(|(k, v)| !k.is_empty() && !k.contains(' ') && k.chars().all(|c| c.is_alphanumeric() || "_-.".contains(c)) && (v.is_empty() || v.starts_with(' '))) }).count();
        let items = lines.iter().filter(|l| l.trim_start().starts_with("- ")).count();
        if lines.len() >= 3 && keys >= 3 && keys + items >= lines.len() - 1 && (trimmed.starts_with("---") || lines.iter().any(|l| l.starts_with("  ") || l.trim_start().starts_with("- "))) { return Some("YAML"); }
    }
    None
}

/// The language of a snippet, "" for a block that only has the marks of code, None for words.
pub fn code_language(t: &str) -> Option<&'static str> {
    let t = t.trim();
    if t.len() < 3 { return None; }
    if let Some(l) = command_language(t) { return Some(l); }
    if let Some(l) = structured_language(t) { return Some(l); }
    let lower = t.to_lowercase();
    let marks = has_marks(t);
    let mut best: Option<(&'static str, u32)> = None;
    for lang in LANGS {
        let mut score: u32 = lang.pats.iter().filter(|(p, _)| hit(t, &lower, lang.nocase, p)).map(|(_, w)| *w as u32).sum();
        if score > 0 && marks { score += 1; }
        if score >= 3 && best.map_or(true, |(_, b)| score > b) { best = Some((lang.name, score)); }
    }
    if let Some((name, _)) = best {
        // A language list is guessed from the text alone: long prose with a couple of keywords must not pass.
        let symbols = t.chars().filter(|c| "{}()[];=<>$/\\|&*#:_".contains(*c)).count();
        if t.len() > 160 && (symbols * 100 / t.len()) < 2 { return None; }
        let ts = name == "JavaScript" && (hit(t, &lower, false, ": string") || hit(t, &lower, false, ": number") || hit(t, &lower, false, "^interface ") || hit(t, &lower, false, "^type ") || hit(t, &lower, false, ": boolean") || hit(t, &lower, false, "<T>") || hit(t, &lower, false, "^enum "));
        return Some(if ts { "TypeScript" } else { name });
    }
    // The old rule: several lines with enough braces and semicolons are code even when no language can be named.
    let lines = t.lines().count();
    let many = t.matches(['{', '}', ';']).count() + t.matches("=>").count() + t.matches("();").count() + t.matches("</").count();
    if lines >= 2 && many >= 3 { return Some(""); }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_need_no_scheme() {
        for t in ["https://youtu.be/abc", "http://a.b", "www.supaste.com", "supaste.com", "github.com/user/repo", "docs.rs/serde/latest", "sub.example.co.id/path?x=1#y", "localhost:3000/app", "WWW.Test.Com", "claude.ai/install.sh", "example.dev:8080",
            // every real ending works bare, not only the common ones
            "kompas.id", "amazon.in", "bbc.co.uk", "toko.blue", "studio.ninja", "my.site.world", "x.com", "t.me", "user.id/profile", "www.anything.xyz"] {
            assert!(link_url(t).is_some(), "{t} is a link");
            assert_eq!(classify(t), "link", "{t}");
        }
        assert_eq!(link_url("www.supaste.com").unwrap(), "https://www.supaste.com");
        assert_eq!(link_url("https://a.b/c").unwrap(), "https://a.b/c");
        assert_eq!(link_url("localhost:3000").unwrap(), "http://localhost:3000");
        for t in ["README.md", "main.rs", "script.sh", "setup.py", "index.html", "app.js", "file.txt", "e.g.", "hello.world", "see www.test.com please", "a@b.com", "user@host.com/path", "https://", "version 1.2.3", "3.14", "node.js", "foo.", "(www.test.com)", "test.com,", "photo.zip", "x.pl",
            "user.id", "item.name", "userProfile.id", "Main.java", "bundle.js.map", "main.tf", "libc.so", "app.config"] {
            assert!(link_url(t).is_none(), "{t} is not a link");
        }
        assert_eq!(classify("a.b@mail.example.com"), "email");
    }

    #[test]
    fn colours_numbers_and_words() {
        assert_eq!(classify("#1a2b3c"), "color");
        assert_eq!(classify("FFF2A5"), "color");
        assert_eq!(classify("28301C"), "color");
        assert_eq!(classify("rgb(79, 157, 255)"), "color");
        assert_eq!(classify("hsl(213, 100%, 65%)"), "color");
        assert_eq!(classify("facade"), "text");
        assert_eq!(classify("123456"), "text");
        assert_eq!(classify("+62 812-3456-7890"), "phone");
        assert_eq!(classify("hello there"), "text");
        assert_eq!(classify("I went to the store."), "text");
        assert_eq!(classify("Please return the book\nthanks a lot"), "text");
        assert_eq!(classify("let me know; thanks"), "text");
        assert_eq!(classify("I import cars from Japan and return them to the shop when they break"), "text");
        assert_eq!(classify("go to the store and buy some milk"), "text");
        assert_eq!(classify("cat is cute"), "text");
        assert_eq!(classify("Meeting moved to Thursday 14:00, room 3. Bring the report and the signed forms."), "text");
        assert_eq!(classify("coba cek clipboard gw sekarang, ada beberapa yang tidak sesuai dengan filter. misalnya links dan code. terkadang mereka masuk ke kategori text."), "text");
    }

    fn lang(t: &str) -> Option<&'static str> { code_language(t) }

    #[test]
    fn one_line_commands() {
        assert_eq!(lang("git status"), Some("Shell"));
        assert_eq!(lang("npm run build"), Some("Shell"));
        assert_eq!(lang("curl -fsSL https://claude.ai/install.sh | bash"), Some("Shell"));
        assert_eq!(lang("iwr -useb https://raw.githubusercontent.com/spicetify/cli/main/install.ps1 | iex"), Some("PowerShell"));
        assert_eq!(lang("Get-ChildItem -Recurse"), Some("PowerShell"));
        assert_eq!(lang("$env:PATH"), Some("PowerShell"));
        assert_eq!(lang("ls -la"), Some("Shell"));
        assert_eq!(lang("cd ~/projects"), Some("Shell"));
        assert_eq!(lang("python manage.py runserver"), Some("Shell"));
        assert_eq!(lang("./run.sh"), Some("Shell"));
        assert_eq!(lang("docker compose up -d"), Some("Shell"));
        assert_eq!(lang("git"), None);
    }

    #[test]
    fn many_languages() {
        let cases: &[(&str, &str)] = &[
            ("fn main() {\n    println!(\"hi\");\n}", "Rust"),
            ("let mut v = Vec::new();\nv.push(1);", "Rust"),
            ("package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"hi\")\n}", "Go"),
            ("def add(a, b):\n    return a + b", "Python"),
            ("class A:\n    def __init__(self):\n        self.x = 1", "Python"),
            ("const x = 1;", "JavaScript"),
            ("items.map(i => i.id)", "JavaScript"),
            ("console.log('hi');", "JavaScript"),
            ("export default function App() {\n  const [a, setA] = useState(0);\n}", "JavaScript"),
            ("interface User {\n  name: string;\n  age: number;\n}", "TypeScript"),
            ("public class Main {\n  public static void main(String[] args) {\n    System.out.println(\"hi\");\n  }\n}", "Java"),
            ("using System;\n\nnamespace App {\n  class P { static void Main() { Console.WriteLine(\"hi\"); } }\n}", "C#"),
            ("#include <iostream>\nint main() {\n  std::cout << \"hi\" << std::endl;\n}", "C++"),
            ("#include <stdio.h>\nint main() {\n  printf(\"hi\");\n  return 0;\n}", "C"),
            ("<?php\necho \"hi\";\n$this->x = 1;", "PHP"),
            ("def hello\n  puts 'hi'\nend", "Ruby"),
            ("import SwiftUI\n\nstruct V: View {\n  var body: some View { Text(\"hi\") }\n}", "Swift"),
            ("fun main() {\n    val x = 1\n    println(x)\n}", "Kotlin"),
            ("void main() {\n  runApp(MyApp());\n}\nclass MyApp extends StatelessWidget {}", "Dart"),
            ("local function f(x)\n  return x\nend", "Lua"),
            ("use strict;\nmy $x = 1;\nprint \"$x\\n\";", "Perl"),
            ("library(ggplot2)\ndf <- data.frame(a = 1:3)", "R"),
            ("defmodule A do\n  def f(x), do: x |> IO.puts()\nend", "Elixir"),
            ("(defn add [a b]\n  (+ a b))", "Clojure"),
            ("SELECT id, name FROM users WHERE id = 1 ORDER BY name;", "SQL"),
            ("select * from users;", "SQL"),
            ("<!DOCTYPE html>\n<html>\n<body><div class=\"a\">x</div></body>\n</html>", "HTML"),
            ("<div class=\"card\"><p>Hi</p></div>", "HTML"),
            (".card {\n  display: flex;\n  padding: 8px;\n}", "CSS"),
            ("@media (max-width: 600px) {\n  body { margin: 0; }\n}", "CSS"),
            ("{\"a\": 1, \"b\": [1, 2]}", "JSON"),
            ("name: app\nversion: 1\nitems:\n  - a\n  - b", "YAML"),
            ("[package]\nname = \"sora\"\nversion = \"1.0\"", "TOML / INI"),
            ("FROM node:20\nWORKDIR /app\nCOPY . .\nRUN npm ci\nCMD [\"node\", \"x.js\"]", "Dockerfile"),
            ("#!/bin/bash\nset -e\necho \"hi\"", "Shell"),
            ("@echo off\nset /p x=Name: \necho %x%", "Batch"),
            ("pragma solidity ^0.8.0;\ncontract A { uint256 public x; }", "Solidity"),
            ("query {\n  user(id: 1) { name }\n}\ntype User {\n  id: ID!\n}", "GraphQL"),
            ("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b", "Diff"),
            ("mov eax, 1\npush ebp\nret", "Assembly"),
            ("<?xml version=\"1.0\"?>\n<project xmlns=\"x\"></project>", "XML"),
        ];
        for (code, want) in cases {
            assert_eq!(code_language(code), Some(*want), "{code}");
            assert_eq!(classify(code), "code", "{code}");
        }
    }

    #[test]
    fn a_block_with_the_marks_is_code_without_a_name() {
        assert_eq!(code_language("if (a) {\n  b();\n}"), Some(""));
        assert_eq!(classify("if (a) {\n  b();\n}"), "code");
    }
}
