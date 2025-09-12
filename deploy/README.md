### How to build an image

We use docker-compose to build an image bundled with postgres database container.
However, one can build app image only using `docker buildx`.
In this case, do not forget to expose required environment variables to container. E.g. `ROCKET_TQ={db_url="postgresql://host/database_name"}`

##### Locally 

* Ensure correct `.env` file with database credentials exists
* Ensure current `pwd` is this folder
* `eval $(ssh-agent)`
* `ssh-add ~/.ssh/your_private_key`
* `docker compose build --ssh default`


##### Github Actions

```
uses: <ssh-key-forwarding-aciton>
  with:
    ssh-private-key: ${{ secrets.SSH_PRIVATE_KEY }}
```

> Some users may need to set `DOCKER_BUILDKIT=1`

### How to launch

* Ensure the same correct `.env` file exists
* Ensure current `pwd` is this folder
* `docker compose up`
