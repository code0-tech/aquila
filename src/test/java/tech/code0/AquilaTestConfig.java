package tech.code0;

import io.lettuce.core.api.StatefulRedisConnection;
import io.micronaut.context.annotation.Factory;
import io.micronaut.context.annotation.Replaces;
import jakarta.inject.Singleton;

import static org.mockito.Mockito.mock;

@Factory
public final class AquilaTestConfig {

    @Singleton
    @Replaces(StatefulRedisConnection.class)
    StatefulRedisConnection<?, ?> redisConnection() {
        return mock(StatefulRedisConnection.class);
    }
}