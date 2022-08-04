# Building the search index with Zola

1. Create a template, which iterates over all pages and creates our JSON structure.

`macros/create_tinysearch_json.html`:

```liquid
{%- macro from_section(section) -%}
{%- set section = get_section(path=section) -%}
{%- for post in section.pages -%}
{%- if not post.draft -%}
{
"title": {{ post.title | striptags | json_encode | safe }},
"url": {{ post.permalink | json_encode | safe }},
"body": {{ post.content | striptags | json_encode | safe }}
}
{%- if not loop.last -%},{%- endif %}
{%- endif -%}
{%- endfor -%}
{%- if section.subsections -%}
,
{%- for subsection in section.subsections -%}
{{ self::from_section(section=subsection) }}
{%- endfor -%}
{%- endif -%}
{%- endmacro from_section -%}
```

`templates/tinysearch_json.html`:

```liquid
{%- import "macros/create_tinysearch_json.html" as create_tinysearch_json -%}
[{{ create_tinysearch_json::from_section(section="_index.md") }}]
```

2. Create a static page using the template.

`content/static/tinysearch_json.md`:

```
+++
path = "data_tinysearch"
template = "tinysearch_json.html"
+++
```

After running `zola build`, the output JSON file should be in `public/json/index.html`.
You can then call tinysearch on the index to create your WASM:

```
tinysearch --optimize --path static public/data_tinysearch/index.html
```
