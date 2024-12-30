# Frederika

Chat Telegram bot that utilizes Gemini API and written in Rust 🚀🚀🚀 

## How to install

```console
cargo install --git 'https://github.com/cult-of-mari/frederika.git'
```

## How to run

Copy the [example configuration](https://github.com/cult-of-mari/frederika/blob/main/example_Config.toml)
and change it as you see fit.
You would need [Gemini API key](https://ai.google.dev/gemini-api/docs/api-key)
and [Telegram Bot token](https://core.telegram.org/bots/tutorial#obtain-your-bot-token) to run it.

```toml
[telegram]
token = "Telegram Token"
names = [ "Bot_callout_1", "Bot_callout_2" ]
cache_size = 5

[gemini]
token = "Gemini Token"
personality="""
Personality Here
"""
```

You can start the bot by running it from cli and providing the config path.

```console
$ frederika -c /path/to/Config.toml
```
