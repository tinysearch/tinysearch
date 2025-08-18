# Building a Search Index for tinysearch with Zola

This guide shows how to create a JSON search index for tinysearch using Zola's template engine (Tera). The process involves creating templates that extract content from all your pages and format it as JSON for tinysearch to process.

## Overview

We'll create:
1. A template that outputs JSON 
2. A content file that generates the search index

## Step 1: Create the Search Index Template

Create `templates/tinysearch_index.html`:

```tera
{% set section = get_section(path="_index.md") %}
[
{%- for post in section.pages -%}
{% if not post.draft %}
{
"title": {{ post.title | striptags | json_encode | safe }},
"url": {{ post.permalink | json_encode | safe }},
"body": "{{ post.content | striptags | replace(from="{", to=" ") | replace(from="}", to=" ") | replace(from='"', to=" ") | replace(from="'", to="") | replace(from="\", to="")  | escape }}"
}{% if not loop.last %},{% endif %}
{% endif %}
{%- endfor -%}
]
```

**What this template does:**
- Gets the root section (`_index.md`) which contains all site pages
- Iterates through all pages in the site
- Skips draft pages 
- Extracts the title, URL, and content for each page
- Uses special filtering for the body content to handle JSON escaping
- Outputs properly formatted JSON array

## Step 2: Create the Search Index Page

Create `content/tinysearch.md`:

```toml
+++
title = "Search Index"
path = "tinysearch.json"
template = "tinysearch_index.html"
date = 2025-01-01
+++
```

**Important notes:**
- The `path` parameter determines the output URL (`tinysearch.json`)
- The `template` parameter specifies which template to use
- The `date` field is required to avoid build warnings
- **About the weird path**: Zola will create `public/tinysearch.json/index.html` instead of `public/tinysearch.json` due to how it handles URLs. This is normal Zola behavior - just ignore the strange nested structure.

## Step 3: Build and Process

1. **Build your Zola site:**
   ```bash
   zola build
   ```

2. **Find the generated JSON:**
   The search index will be at `public/tinysearch.json/index.html` (yes, that's a weird path, but it's how Zola works)

3. **Run tinysearch:**
   ```bash
   tinysearch --optimize --path static public/tinysearch.json/index.html
   ```

## Customization Options

### Including Specific Sections Only

To limit indexing to specific sections, modify the macro call:

```tera
[{{ tinysearch::extract_content(section="blog/_index.md") }}]
```

### Adding Metadata

You can extend the macro to include additional metadata:

```tera
{
  "title": {{ page.title | striptags | json_encode | safe }},
  "url": {{ page.permalink | json_encode | safe }},
  "body": {{ page.content | striptags | json_encode | safe }},
  "date": {{ page.date | json_encode | safe }},
  "tags": {{ page.taxonomies.tags | json_encode | safe }}
}
```

## Troubleshooting

### Empty Output
- Check that your sections contain non-draft pages
- Verify the section path in the macro call matches your content structure

### JSON Syntax Errors
- Ensure proper comma placement between items
- Use `json_encode` filter for all dynamic content
- Test the generated JSON with a validator

### Build Errors
- Check that all template syntax is correct (Tera uses `{%` and `{{` syntax)

This setup will create a comprehensive search index that tinysearch can process into an efficient WebAssembly search module for your Zola site.
