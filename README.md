# k-agent

Kubernetes agent for Odin API.
This rust agent is made for deploying, watching and managing Odin applications in a Kubernetes Cluster (K8s, K3s).

## How to deploy a new Kubernetes Agent ? 

The deployment is done in two steps :
- First you register it and get the dedicated UUID (needed for encryption)
- Then you can launch it with the `AGENT_UUID` environment variable that will reach the `ODIN_HUB_URL`.

### 1. Record and get UUID

In the first launch, **do not set** the `AGENT_UUID` variable but set the proper `ODIN_HUB_URL` & `AGENT_LABEL`.



### 2. Launch the agent

Now that you have the UUID, you can set the `AGENT_UUID` and launch the agent again. It will now trigger the every 5 sec to tell its status and get the wanted applications state.