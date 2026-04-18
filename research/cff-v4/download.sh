#!/bin/bash
# Re-download research PDFs (run from this directory)
set -e
declare -A urls=(
  ["01-chisel-trace-informed.pdf"]="https://bmarwritescode.github.io/assets/pdf/chisel.pdf"
  ["03-banescu-resilience.pdf"]="https://www.usenix.org/system/files/conference/usenixsecurity17/sec17-banescu.pdf"
  ["04-vb2023-cff.pdf"]="https://www.virusbulletin.com/uploads/pdf/conference/vb2023/papers/Dont-flatten-yourself-restoring-malware-with-Control-Flow-Flattening-obfuscation.pdf"
  ["06-vm-deobf.pdf"]="https://cis.temple.edu/~qzeng/papers/deobfuscation-icics2017.pdf"
  ["07-synth-symexec.pdf"]="https://essay.utwente.nl/fileshare/file/79934/Coniglio_MA_Computer_Science.pdf"
  ["08-opaque-predicates.pdf"]="https://th0mas.nl/downloads/thesis/thesis.pdf"
  ["09-android-native.pdf"]="https://yaoguopku.github.io/papers/Kan-ICSEp-19.pdf"
  ["10-jsimplifier-ndss26.pdf"]="https://www.ndss-symposium.org/wp-content/uploads/2026-f2198-paper.pdf"
  ["11-cascade-google.pdf"]="https://arxiv.org/pdf/2507.17691"
  ["12-auto-simplify-js.pdf"]="https://www.cs.arizona.edu/~genlu/pub/js-deobf-web.pdf"
  ["13-safe-deobs.pdf"]="https://adrian-herrera.com/assets/publications/safe-deobs.pdf"
  ["14-invoke-deobf.pdf"]="https://snowroll.github.io/papers/Invoke_Deobfuscation_DSN_2022.pdf"
  ["16-cmu-dataflow.pdf"]="https://www.cs.cmu.edu/afs/cs/academic/class/15745-s03/public/lectures/L4_handouts.pdf"
  ["17-advanced-binary-deobf.pdf"]="https://raw.githubusercontent.com/malrev/ABD/master/Advanced-Binary-Deobfuscation.pdf"
  ["18-schloegel-dissertation.pdf"]="https://mschloegel.me/paper/schloegel2024dissertation.pdf"
  ["20-hexrays-virt.pdf"]="https://www.virusbulletin.com/uploads/pdf/conference/vb2023/papers/Deobfuscating-virtualized-malware-using-Hex-Rays-Decompiler.pdf"
)
for name in "${!urls[@]}"; do
  [ -f "$name" ] || curl -sL -o "$name" --max-time 30 -H "User-Agent: Mozilla/5.0" "${urls[$name]}"
done
for pdf in *.pdf; do pdftotext -layout "$pdf" "${pdf%.pdf}.txt"; done
echo "✓ downloaded + converted"
