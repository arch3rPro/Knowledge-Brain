# ADR-0010: Pure Rust built-in document extractors

- Status: accepted / implemented
- Class: architecture

## Problem

HTML, EPUB and DOCX sources need searchable text and stable citation positions without making a platform package, Java runtime, office suite or agent-specific tool part of the basic installation. PDF extraction has a substantially different quality and complexity profile.

## Decision

The `kb` binary extracts HTML, EPUB and DOCX with Rust libraries behind the shared `Extractor` contract. Cache files and capabilities report the selected extractor identity. EPUB and DOCX reject encrypted, duplicate, unsafe or excessive archive entries before parsing their contents.

PDF bytes produce `metadata_only`. PDF text extraction and OCR remain future extractor implementations and do not block the built-in document formats.

## Alternatives considered

**Require Apache Tika or another external program.** Mature format coverage would arrive sooner, but installation, process management and runtime updates would become part of the basic cross-platform workflow.

**Implement PDF extraction in the same stage.** A limited text-layer parser is possible, but PDF font mapping, reading order and scanned documents would dominate the stage without improving HTML, EPUB or DOCX behavior.

**Treat every document as metadata-only.** This preserves source bytes but prevents direct source search from serving common human-authored documents.

## Consequences

- HTML, EPUB and DOCX extraction needs no external executable and uses the same implementation on Windows, macOS and Linux.
- Extracted titles, links and format-native locations are available to review, cache and direct source search consumers.
- EPUB and DOCX parsing accepts semantic text loss instead of attempting visual layout reconstruction.
- Rust parser dependencies and malformed-document fixtures become part of ongoing maintenance.
- PDF sources remain preserved and identifiable but their body text is not searchable until a separate extractor is implemented.
