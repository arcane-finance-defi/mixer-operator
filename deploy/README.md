### How to build an image

##### Locally 

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