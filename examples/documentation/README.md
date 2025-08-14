# Documentation Search Example

This example demonstrates how to use TinySearch for a comprehensive documentation website with organized content and metadata.

## Configuration  

The `tinysearch.toml` file configures the search to:

- **Index**: `title`, `content`, `section`, and `keywords` for thorough documentation coverage
- **Store as metadata**: `version`, `last_updated`, `contributor`, `difficulty`, `type`
- **Use**: `doc_url` as the link field for each documentation page

## Sample Data

The `docs.json` file contains 11 documentation pages covering:
- **Getting Started**: Installation and quickstart guide
- **Configuration**: Schema setup and customization options
- **Reference**: JSON format, CLI commands, API documentation
- **Integration**: WebAssembly, static site generators, performance
- **Support**: Troubleshooting, examples, contributing guidelines

Each page includes realistic technical content, version information, difficulty levels, and contributor details.

## Usage

From this directory, run:

```bash
# Generate the search index
tinysearch -m storage -p ./output docs.json

# Search for configuration help
tinysearch -m search -S "configuration schema" -N 3 ./output/storage

# Find integration guides
tinysearch -m search -S "webassembly integration" -N 2 ./output/storage

# Look for troubleshooting info
tinysearch -m search -S "errors troubleshooting" -N 5 ./output/storage
```

## For Documentation Sites

Generate WASM files for your docs:

```bash
# Development version with demo
tinysearch -m wasm -p ./wasm_output docs.json

# Production version for deployment
tinysearch --release -m wasm -p ./wasm_output docs.json
```

## Search Examples

Users can search for:
- **Features**: "configuration", "webassembly", "json format", "cli commands"
- **Sections**: "getting started", "api reference", "troubleshooting"
- **Difficulty**: "beginner", "intermediate", "advanced"
- **Content Types**: "guide", "reference", "examples"
- **Technical Terms**: "wasm", "javascript", "integration", "performance"
- **Topics**: "installation", "optimization", "static site generators"

Search results include version numbers, last updated dates, difficulty levels, and content types to help users find the most relevant and up-to-date information for their needs.

## Documentation Features

This example showcases TinySearch capabilities for documentation sites:
- **Multi-level organization** with sections and subsections
- **Version tracking** for maintaining multiple doc versions
- **Contributor attribution** for community-driven documentation
- **Content classification** by difficulty and type
- **Comprehensive keyword indexing** for precise search results