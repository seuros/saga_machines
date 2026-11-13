# frozen_string_literal: true
require 'minitest/autorun'
require 'saga_machines'

module SagaHelpers
  include SagaMachines

  ID = SagaMachines::SagaId.new(7)

  # Everything a saga holds: a refused call must leave it unchanged.
  def state(saga)
    [saga.steps, saga.snapshot, saga.pending]
  end

  def assert_unchanged(saga)
    before = state(saga)
    yield
    assert_equal before, state(saga)
  end

  # Issue the next action and resolve it with `outcome`.
  def run_step(saga, outcome)
    action = saga.next_action
    assert_instance_of Action, action
    assert_equal :resolved, saga.resolve(action, outcome)
    action
  end

  def action(step, kind, attempt, saga: ID)
    Action.new(ActionKey.new(saga, step, kind), AttemptId.new(attempt))
  end
end
