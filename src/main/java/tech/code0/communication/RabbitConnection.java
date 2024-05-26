package tech.code0.communication;

import com.rabbitmq.client.Connection;
import com.rabbitmq.client.ConnectionFactory;
import lombok.Getter;
import tech.code0.configuration.AquilaConfiguration;

import java.util.logging.Logger;

public class RabbitConnection {

    @Getter private final Connection connection;

    private final Logger logger;

    public RabbitConnection(AquilaConfiguration aquilaConfiguration) {
        this.logger = Logger.getLogger(RabbitConnection.class.getName());

        this.logger.info("Initializing Rabbit connection");
        this.connection = createConnection(aquilaConfiguration);
    }

    private Connection createConnection(AquilaConfiguration aquilaConfiguration) {
        final var connectionFactory = new ConnectionFactory();
        connectionFactory.setHost(aquilaConfiguration.getRabbitMQHost());
        connectionFactory.setPort(aquilaConfiguration.getRabbitMQPort());

        try {
            this.logger.info("Connected to RabbitMQ");
            return connectionFactory.newConnection();
        } catch (Exception exception) {
            this.logger.warning(STR."Connection to RabbitMQ failed: \{exception.getMessage()}");
            throw new RuntimeException(exception);
        }
    }

    public void close() {
        try {
            this.logger.warning("Closing RabbitMQ connection");
            this.connection.close();
        } catch (Exception exception) {
            this.logger.severe(STR."Failed to close RabbitMQ connection: \{exception.getMessage()}");
        }
    }
}