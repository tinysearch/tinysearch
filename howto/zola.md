# Building the search index with Zola

1. Create a template, which iterates over all pages and creates our JSON structure.

`macros/create_data.html`:

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

`templates/json.html`:

```liquid
{%- import "macros/create_data.html" as create_data -%}
[{{ create_data::from_section(section="_index.md") }}]
```

2. Create a static page using the template.

`content/static/json.md`:

```
+++
path = "json"
template = "json.html"
+++
```

After running `zola build`, the output JSON file should be in `public/json/index.html`.
You can then call tinysearch on the index to create your WASM:

```
tinysearch --optimize --path static public/json/index.html
```
