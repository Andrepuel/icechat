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
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait,
    IntoActiveModel, ModelTrait, Order, QueryFilter, QueryOrder, Statement,
};

impl SyncDataSource for DatabaseTransaction {
    type Ctx = i32;

    fn next(
        &mut self,
        channel_id: i32,
        (min_initial, min_global): (i32, i32),
    ) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>> {
        async move {
            let initial_sync = initial_sync::Entity::find()
                .filter(initial_sync::Column::Channel.eq(channel_id))
                .filter(initial_sync::Column::Id.gt(min_initial))
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
                .filter(entity::entity::sync::Column::Id.gt(min_global))
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
                    remove_old_patches(self).await?;

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

async fn remove_old_patches(trans: &DatabaseTransaction) -> DatabaseResult<()> {
    let done_sync = channel::Entity::find()
        .order_by(channel::Column::SyncIndex, Order::Asc)
        .one(trans)
        .await?;

    let Some(done_sync) = done_sync else { return Ok(()); };

    trans
        .execute(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "DELETE FROM sync WHERE id <= ?;",
            [done_sync.sync_index.into()],
        ))
        .await?;

    Ok(())
}
