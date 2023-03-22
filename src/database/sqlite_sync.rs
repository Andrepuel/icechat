use super::{
    error::DatabaseResult,
    sync::{SyncData, SyncDataId, SyncDataSource},
};
use entity::{
    entity::{channel, initial_sync},
    patch::Patch,
};
use futures_util::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, IntoActiveModel,
    ModelTrait, QueryFilter, QueryOrder,
};

impl SyncDataSource for DatabaseTransaction {
    type Ctx = i32;

    fn next(&mut self, channel_id: i32) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>> {
        async move {
            let initial_sync = initial_sync::Entity::find()
                .filter(initial_sync::Column::Channel.eq(channel_id))
                .order_by(initial_sync::Column::Id, sea_orm::Order::Asc)
                .one(self)
                .await?;

            if let Some(initial_sync) = initial_sync {
                let patch: Patch = bincode::deserialize(&initial_sync.payload).unwrap();

                return Ok(Some(SyncData {
                    id: SyncDataId::InitialSync(initial_sync.id),
                    payload: patch,
                }));
            }

            let channel = channel::Entity::find_by_id(channel_id).one(self).await?;
            let Some(channel) = channel else { return Ok(None); };
            let sync = entity::entity::sync::Entity::find()
                .filter(entity::entity::sync::Column::Id.gt(channel.sync_index))
                .one(self)
                .await?;

            if let Some(sync) = sync {
                let patch: Patch = bincode::deserialize(&sync.payload).unwrap();

                return Ok(Some(SyncData {
                    id: SyncDataId::Global(sync.id),
                    payload: patch,
                }));
            }

            Ok(None)
        }
        .boxed_local()
    }

    fn ack(&mut self, channel_id: i32, id: SyncDataId) -> LocalBoxFuture<DatabaseResult<()>> {
        async move {
            match id {
                SyncDataId::Global(id) => {
                    let channel = channel::Entity::find_by_id(channel_id).one(self).await?;
                    let Some(channel) = channel else { return Ok(()); };

                    if channel.sync_index >= id {
                        return Ok(());
                    }

                    let channel = channel::ActiveModel {
                        sync_index: ActiveValue::Set(id),
                        ..channel.into_active_model()
                    };
                    channel.save(self).await?;

                    Ok(())
                }
                SyncDataId::InitialSync(id) => {
                    let initial_sync = initial_sync::Entity::find_by_id(id).one(self).await?;
                    let Some(initial_sync) = initial_sync else { return Ok(()); };

                    if initial_sync.channel != channel_id {
                        return Ok(());
                    }

                    initial_sync.delete(self).await?;

                    Ok(())
                }
            }
        }
        .boxed_local()
    }

    fn merge(
        &mut self,
        _channel_id: i32,
        data: SyncData,
    ) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>> {
        async move {
            let merged = data.payload.merge(self).await;
            let merged = merged.map(|payload| SyncData {
                id: data.id,
                payload,
            });

            Ok(merged)
        }
        .boxed_local()
    }

    fn save(&mut self, _channel_id: i32, data: SyncData) -> LocalBoxFuture<DatabaseResult<()>> {
        async move {
            let payload = bincode::serialize(&data.payload).unwrap();

            entity::entity::sync::ActiveModel {
                id: ActiveValue::NotSet,
                payload: ActiveValue::Set(payload),
            }
            .save(self)
            .await?;

            Ok(())
        }
        .boxed_local()
    }
}
