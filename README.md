This branch is an integration attempt of bevy_rapier2d and bevy_ggrs. 

The current approach is to run rapier inside of the rollback schedule,
deserializing and serializing the RapierContext before and after each
rollback execution.

The problem is that RapierContext is huge as it tracks the whole physics
state, which contains the static colliders.

A possible solution is to track only the rigidbodies and the contact 
graph and leave the static state.
