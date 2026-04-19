---
title: Aquila Setup Guide
---

Use this guide to install and configure Aquila.

## Setup Options

### Using Docker


### Setting Up a Virtual Development Environment (Preferred)

[Visit Setup Guide](https://docs.code0.tech/general/install/)

Use Docker Compose to start the application. Make sure Aquila is stopped while you are developing locally.  
Alternatively, set `COMPOSE_PROFILES=ide` to exclude runtime services (you will need to start NATS & Aquila manually).

### Manual Installation

1. **Clone Aquila**  
   Clone the repository to your local machine.
2. **Set Up Environment Variables**  
   Configure the `.env` file in the root folder with the required settings.
3. **Ensure Required Services Are Running**
   - **NATS**:
     - Ensure a NATS instance is reachable.
     - Enable JetStream.
     - For help, refer to the [NATS documentation](https://docs.nats.io/running-a-nats-service/introduction/installation).
   - **Sagittarius**: Ensure a Sagittarius instance is reachable.
4. **Start the Application**  

---

## Environment Variables

Below is a list of environment variables for configuring Aquila. The configuration is split into common variables and mode-specific variables.

### Common (Static + Dynamic)

| Name                    | Description                                                                                         | Default                         |
|-------------------------|-----------------------------------------------------------------------------------------------------|---------------------------------|
| `MODE`                  | Application mode. `static` starts from a local flow file. Any non-`static` mode runs dynamic mode. | `static`                        |
| `ENVIRONMENT`           | Logging/behavior environment (`development`, `staging`, `production`).                              | `development`                   |
| `NATS_URL`              | URL of the NATS instance Aquila connects to.                                                       | `nats://localhost:4222`         |
| `NATS_BUCKET`           | Name of the NATS KV bucket used to store flows.                                                     | `flow_store`                    |
| `GRPC_HOST`             | Hostname for the Aquila gRPC server.                                                                | `127.0.0.1`                     |
| `GRPC_PORT`             | Port for the Aquila gRPC server.                                                                    | `8081`                          |
| `WITH_HEALTH_SERVICE`   | If `true`, Aquila enables the gRPC health service.                                                  | `false`                         |
| `SERVICE_CONFIG_PATH`   | Path to the service configuration file used for action/runtime tokens and default action configs. | `./service.configuration.json` |

### Static Mode

Set `MODE=static` to start Aquila from a local flow file and insert flows into the NATS KV store.

| Name                 | Description                                             | Default            |
|----------------------|---------------------------------------------------------|--------------------|
| `FLOW_FALLBACK_PATH` | Path to the flow JSON file loaded on startup.           | `./flowExport.json` |

### Dynamic Mode

Dynamic mode keeps flows updated by streaming from Sagittarius. Any non-`static` mode value will
start dynamic mode (for example `MODE=hybrid` if supported by your `code0_flow` version).

| Name              | Description                                                                       | Default                   |
|-------------------|-----------------------------------------------------------------------------------|---------------------------|
| `SAGITTARIUS_URL` | URL of the Sagittarius instance Aquila connects to for flow/action configuration. | `http://localhost:50051`  |
| `RUNTIME_TOKEN`   | Token used to authenticate Aquila with Sagittarius.                               | `default_session_token`   |

---

## Service Configuration File

To add services like `Taurus`, `Draco`, or an `Action` to the runtime, the service must be configured in Aquila to ensure it is authorized.

`SERVICE_CONFIG_PATH` points to a JSON file that defines allowed runtime/action tokens and optional
default action configurations. This file is loaded on startup; if it is missing or invalid, Aquila
starts with an empty service configuration.

By default, Aquila includes a configuration that is preconfigured for runtime services.

Default:

```json
{
	"actions": [],
	"runtimes": [
		{
			"identifier": "taurus",
			"token": "HsCEzbCuaUtUGSCrvwsSbJSlS2HH6TrW0ZeEKUZGTiOH8vPEZxyAEOx974Ku72l4"
		},
		{
			"identifier": "draco-rest",
			"token": "SBO3dRKmhszmGH6KxpgKoYGp0gBfgWqV6WEiKtMxldyeWiYLqJx6vwLuVLKRhu8H"
		},
		{
			"identifier": "draco-cron",
			"token": "VuTFgCj1PO6yr8smk43XLmeTUtlyKa2wjA0zvmz7WZDtgfXC62Ypd1b8fjJl8HvI"
		}
	]
}
```
You can add as many runtimes as you need. To add an `Action`, add an entry to `actions`. To provide a configuration for an `Action`, add `config` objects to that action.

```json
{
  "actions": [
    {
      "token": "action_token",
      "identifier": "discord",
      "configs": [
        {
          "project_id": 1,
          "configs": [
            {
              "identifier": "send_message",
              "value": {
                "channel_id": "123456789012345678",
                "content": "Hello from bot"
              }
            }
          ]
        }
      ]
    }
  ],
  "runtimes": []
}
```
