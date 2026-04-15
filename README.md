# k-agent satellite

Kubernetes agent for Odin API.
This rust agent is made for deploying, watching and managing Odin applications in a Kubernetes Cluster (K8s, K3s).

## How to deploy a new Kubernetes Agent ? 

The deployment is done in two steps :
- First you register it and get the dedicated UUID (needed for encryption)
- Second, get the `MOTHERSHIP_AUTH_TOKEN` in the mothership api logs and put it in the agent **.env**.
- Then you can launch it with the `AGENT_UUID` environment variable that will reach the `MOTHERSHIP_URL`.

### 1. Record and get UUID

In the first launch, **do not set** the `AGENT_UUID` variable but set the proper `MOTHERSHIP_URL` & `AGENT_LABEL`.

### 2. Launch the agent

Now that you have the UUID, you can set the `AGENT_UUID` / `MOTHERSHIP_URL` and launch the agent again. It will now trigger the every 5 sec to tell its status and get the wanted applications state.

