# Docker Hub Deployment

This repository uses GitHub Actions to automatically build and push Docker images to Docker Hub.

## Image Name
`raynkami/grpc.trader`

## Auto-Generated Tags

The workflow automatically generates the following tags:

- **Branch-based tags**: `main`, `develop`, etc.
- **Commit SHA tags**: `main-abc1234`, `develop-def5678` (short SHA)
- **Semver tags** (from git tags): `v1.0.0`, `1.0`, `1`
- **Latest tag**: Applied to images from the main branch
- **PR tags**: `pr-123` for pull requests

## Setup Required

To enable Docker Hub deployment, add the following secrets to your GitHub repository:

1. Go to **Settings** → **Secrets and variables** → **Actions**
2. Add these repository secrets:
   - `DOCKERHUB_USERNAME`: Your Docker Hub username
   - `DOCKERHUB_TOKEN`: Your Docker Hub access token (create at https://hub.docker.com/settings/security)

## Triggering Builds

Builds are triggered on:
- Push to `main` or `develop` branches
- Creating a git tag (e.g., `v1.0.0`)
- Opening/updating a pull request to `main`

## Example Usage

```bash
# Pull the latest image
docker pull raynkami/grpc.trader:latest

# Pull a specific version
docker pull raynkami/grpc.trader:v1.0.0

# Pull a specific commit
docker pull raynkami/grpc.trader:main-abc1234
```
