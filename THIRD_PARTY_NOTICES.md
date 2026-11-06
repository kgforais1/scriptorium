# Third-party notices

Scriptorium statically links two C libraries into the app binary. This file lists each one with its bundled version, its upstream, and its licence text. It also records an advisory check made on 2026-11-01.

The licences of the Rust crates and npm packages that Scriptorium depends on are not repeated here. Cargo and pnpm list them: run `cargo metadata` in `src-tauri/`, or `pnpm licenses list`. Versions below come from `src-tauri/Cargo.lock` and from the bundled C source in each crate.

## OpenJPEG

- Upstream: <https://www.openjpeg.org/> (<https://github.com/uclouvain/openjpeg>)
- Bundled version: 2.5.4 (from `config/opj_config.h` and `opj_config_private.h` in the crate)
- Bundled by: the `openjpeg-sys` crate 1.0.12, patched to a [fork commit](https://github.com/kgrizz-git/openjpeg-sys/commit/1e04bc79c120ca63b2fcc53a0f20166316234edd) that bundles OpenJPEG 2.5.4 (`[patch.crates-io]` in `src-tauri/Cargo.toml`), pulled in by `jpeg2k` 0.10.1 with its `openjpeg-sys` feature
- Used for: decoding JPEG 2000 scans
- Licence: BSD-2-Clause

The fork changes only the vendored C sources, the two config headers, and the version constant in `src/ffi.rs`. The bindings are otherwise those of 1.0.12, because OpenJPEG 2.5.4 has no API changes.

Licence text, verbatim from the crate's `vendor/LICENSE` (trailing whitespace removed):

~~~~text
/*
 * The copyright in this software is being made available under the 2-clauses
 * BSD License, included below. This software may be subject to other third
 * party and contributor rights, including patent rights, and no such rights
 * are granted under this license.
 *
 * Copyright (c) 2002-2014, Universite catholique de Louvain (UCL), Belgium
 * Copyright (c) 2002-2014, Professor Benoit Macq
 * Copyright (c) 2003-2014, Antonin Descampe
 * Copyright (c) 2003-2009, Francois-Olivier Devaux
 * Copyright (c) 2005, Herve Drolon, FreeImage Team
 * Copyright (c) 2002-2003, Yannick Verschueren
 * Copyright (c) 2001-2003, David Janssens
 * Copyright (c) 2011-2012, Centre National d'Etudes Spatiales (CNES), France
 * Copyright (c) 2012, CS Systemes d'Information, France
 *
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS `AS IS'
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
~~~~

## Little CMS (LittleCMS)

- Upstream: <https://www.littlecms.com/> (<https://github.com/mm2/Little-CMS>)
- Bundled version: 2.19.1 (stated in the `lcms2-sys` README; `lcms2.h` reports `LCMS_VERSION 2190`, which is major and minor only)
- Bundled by: the `lcms2-sys` crate 4.0.7, pulled in by `lcms2` 6.2.0 with its `static` feature
- Used for: ICC colour management
- Licence: MIT

Licence text, verbatim from the crate's `vendor/LICENSE` (trailing whitespace removed):

~~~~text
MIT License

Copyright (c) 2023 Marti Maria Saguer

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject
to the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
~~~~

## Advisory check (2026-11-01)

I searched the web for published advisories against each bundled version on 2026-11-01. The NVD page for CVE-2025-54874 did not render in the fetch tool, so its details come from distribution and CVE-aggregator pages.

- OpenJPEG 2.5.4 has no known advisory. CVE-2025-54874, an out-of-bounds heap write in `opj_jp2_read_header`, affects 2.5.3 and earlier and is fixed in 2.5.4 (upstream commit f809b80c). The newest `openjpeg-sys` on crates.io, 1.0.12, still bundles 2.5.3, so `src-tauri/Cargo.toml` patches it to a fork commit with 2.5.4 (2026-11-03). The patch goes once an `openjpeg-sys` release bundles 2.5.4.
- Little CMS 2.19.1 has no known advisory. CVE-2026-41254 and CVE-2026-42798 are integer overflows fixed in 2.19, so 2.19.1 is not affected.

Re-run this check when a bundled version changes and before each release.
