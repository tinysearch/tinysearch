# Building the search index with Pelican

1. Create a template, which iterates over all articles and creates our JSON structure.

`templates/json.html`:

```jinja
[
{%- for article in articles -%}
{% if article.status != "draft" %}
{
"title": {{ article.title | striptags | tojson | safe }},
"url": {{ article.url | tojson | safe }},
"body": {{ article.content | striptags | tojson | safe }}
}{% if not loop.last %},{% endif %}
{% endif %}
{%- endfor -%}
]
```

2. Create a static page using the template.

`content/pages/json.md`:

```
Title: JSON
Template: json
Slug: json
```

After running `pelican content`, the output JSON file should be in `output/pages/json.html`.
You can then call tinysearch on the index to create your WASM:

```
tinysearch --optimize --path output output/pages/json.html
```
